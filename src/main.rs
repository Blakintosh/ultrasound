use clap::Parser;

mod env;
mod tables;
mod riff;
mod flac;
mod decoded_audio;
mod units;
mod converter;
mod source_asset_cache;
mod asset_types;
mod bank;
mod string_hash;
mod obtainer;
mod converted_asset_cache;
mod sound_zone_config;
mod duk;
mod music;
mod sound_data_snapshot;
mod filespec;
mod sound_zone;
mod update_bank;

use env::Env;
use sound_data_snapshot::SoundDataSnapshot;
use sound_zone::SoundZone;
use sound_zone_config::SoundZoneConfig;
use update_bank::update_bank;

#[derive(Parser)]
struct Args {
    /// Enable detailed output
    #[arg(long)]
    verbose: bool,

    /// Skip dependency checking
    #[arg(long)]
    skip_deps: bool,

    /// Raw positional arguments from mod tools
    raw_args: Vec<String>
}

enum Action {
    ZoneSources { platform: String, languages: Vec<String>, zones: Vec<String> },
    SoundZone { zone: String, platform: String, languages: Vec<String> },
    Production { platform: String, language: String },
    All,
}

fn main() {
    let args = Args::parse();

    let env = Env::new(&args.raw_args[1], &args.raw_args[4])
        .expect("Failed to initialize environment");

    let mut snapshot = SoundDataSnapshot::new(env)
        .expect("Failed to build sound data snapshot");

    let action = parse_action(&snapshot, &args.raw_args).expect("Failed to parse action");

    if let Err(e) = run(&mut snapshot, action) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn run(snapshot: &mut SoundDataSnapshot, action: Action) -> Result<(), String> {
    match action {
        Action::SoundZone { zone, platform, languages } => {
            for lang in &languages {
                single_zone(snapshot, &zone, &platform, lang)?;
            }
        }
        Action::ZoneSources { platform, languages, zones } => {
            for zone in &zones {
                for lang in &languages {
                    single_zone(snapshot, zone, &platform, lang)?;
                }
            }
        }
        Action::Production { platform, language } => {
            println!("Production not yet supported (platform={}, language={})", platform, language);
        }
        Action::All => {
            println!("'all' action not yet supported");
        }
    }
    Ok(())
}

fn single_zone(
    snapshot: &mut SoundDataSnapshot,
    zone_name: &str,
    platform: &str,
    language: &str,
) -> Result<(), String> {
    let config_path = snapshot
        .env
        .get_sound_zone_config_dir()
        .join(format!("{}.szc", zone_name));
    let config = SoundZoneConfig::load(&config_path)?;

    let zone = SoundZone::generate(snapshot, &config, platform, language)?;
    println!(
        "zone '{}' [{}/{}]: {} loaded, {} streamed",
        zone.name,
        platform,
        language,
        zone.loaded_files.len(),
        zone.streamed_files.len(),
    );

    // Build both banks concurrently — they share the same rayon pool for
    // FLAC encoding, so the win is mostly from eliminating the gap between
    // the two sequential runs rather than 2x throughput.
    let snap: &SoundDataSnapshot = snapshot;
    let (r_loaded, r_streamed) = rayon::join(
        || update_bank(snap, &zone, platform, language, false),
        || update_bank(snap, &zone, platform, language, true),
    );
    r_loaded?;
    r_streamed?;
    Ok(())
}

fn parse_action(snapshot: &SoundDataSnapshot, raw: &[String]) -> Result<Action, String> {
    if raw.is_empty() {
        return Err("Missing action. Specify one of: sound_zone, zone_sources, zone_source, production or all.".to_string());
    }
    if raw.len() < 4 {
        return Err("Not enough arguments.".to_string());
    }

    match raw[3].as_str() {
        "zone_source" | "zone_sources" => {
            let locale_names: Vec<&str> = snapshot.locales.iter().map(|l| l.name.as_str()).collect();

            let mut languages = Vec::new();
            let mut zones = Vec::new();
            for arg in &raw[5..] {
                if locale_names.contains(&arg.as_str()) {
                    languages.push(arg.clone());
                } else {
                    zones.push(arg.clone());
                }
            }

            Ok(Action::ZoneSources {
                platform: raw[0].clone(),
                languages,
                zones,
            })
        }
        "sound_zone" => Ok(Action::SoundZone {
            zone: raw.get(5).ok_or("Missing zone")?.clone(),
            platform: raw[0].clone(),
            languages: raw[6..].to_vec(),
        }),
        "production" => Ok(Action::Production {
            platform: raw[0].clone(),
            language: raw.get(5).ok_or("Missing language")?.clone(),
        }),
        "all" => Ok(Action::All),
        other => Err(format!("Unknown action: {}", other)),
    }
}
