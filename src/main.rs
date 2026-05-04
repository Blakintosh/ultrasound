use clap::Parser;

mod ambient_bsp;
mod asset_types;
mod bank;
mod converted_asset_cache;
mod converter;
mod decoded_audio;
mod duk;
mod env;
mod filespec;
mod flac;
mod music;
mod obtainer;
mod ogg;
mod riff;
mod sound_data_snapshot;
mod sound_zone;
mod sound_zone_config;
mod source_asset_cache;
mod string_hash;
mod sz_writer;
mod tables;
mod units;
mod update_bank;

use env::Env;
use sound_data_snapshot::SoundDataSnapshot;
use sound_zone::SoundZone;
use sound_zone_config::SoundZoneConfig;
use update_bank::update_bank;

const RAW_ARG_USAGE: &str = "Expected raw args: <platform> <platform_working_dir> <unused> <action> <source_dir> [action args...]";

#[derive(Parser)]
struct Args {
    /// Enable detailed output
    #[arg(long)]
    verbose: bool,

    /// Skip dependency checking
    #[arg(long)]
    skip_deps: bool,

    /// Raw positional arguments from mod tools
    raw_args: Vec<String>,
}

#[derive(Debug)]
enum Action {
    ZoneSources {
        platform: String,
        languages: Vec<String>,
        zones: Vec<String>,
    },
    SoundZone {
        zone: String,
        platform: String,
        languages: Vec<String>,
    },
    Production {
        platform: String,
        language: String,
    },
    All,
}

fn main() {
    if let Err(e) = try_main() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

fn try_main() -> Result<(), String> {
    let args = Args::parse();
    let (platform_working_dir, source_dir) = env_args(&args.raw_args)?;

    let env = Env::new(platform_working_dir, source_dir)?;
    let mut snapshot = SoundDataSnapshot::new(env)?;

    let action = parse_action(&snapshot, &args.raw_args)?;

    run(&mut snapshot, action)
}

fn env_args(raw: &[String]) -> Result<(&str, &str), String> {
    if raw.len() < 5 {
        return Err(format!("Not enough arguments. {}", RAW_ARG_USAGE));
    }
    Ok((&raw[1], &raw[4]))
}

fn run(snapshot: &mut SoundDataSnapshot, action: Action) -> Result<(), String> {
    match action {
        Action::SoundZone {
            zone,
            platform,
            languages,
        } => {
            for lang in &languages {
                single_zone(snapshot, &zone, &platform, lang)?;
            }
        }
        Action::ZoneSources {
            platform,
            languages,
            zones,
        } => {
            for zone in &zones {
                for lang in &languages {
                    single_zone(snapshot, zone, &platform, lang)?;
                }
            }
        }
        Action::Production { platform, language } => {
            println!(
                "Production not yet supported (platform={}, language={})",
                platform, language
            );
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

    // Per-zone .sz sidecars (alias / reverb / ambient / ducklist / musiclist).
    // These are the inputs the engine and downstream zone-build steps read,
    // so they must land before the banks are deployed. Bank-derived sidecars
    // (memory / assetcount / assets) are emitted by `update_bank` itself.
    let locale = snapshot
        .get_locale(language)
        .cloned()
        .ok_or_else(|| format!("unknown language '{}'", language))?;
    zone.write_outputs(&snapshot.env, &config, &locale)?;

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
    let locale_names: Vec<&str> = snapshot.locales.iter().map(|l| l.name.as_str()).collect();
    parse_action_with_locales(&locale_names, raw)
}

fn parse_action_with_locales(locale_names: &[&str], raw: &[String]) -> Result<Action, String> {
    let action = raw
        .get(3)
        .ok_or_else(|| format!("Missing action. {}", RAW_ARG_USAGE))?;

    match action.as_str() {
        "zone_source" | "zone_sources" => {
            let mut languages = Vec::new();
            let mut zones = Vec::new();
            for arg in raw.get(5..).unwrap_or(&[]) {
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
            languages: raw.get(6..).unwrap_or(&[]).to_vec(),
        }),
        "production" => Ok(Action::Production {
            platform: raw[0].clone(),
            language: raw.get(5).ok_or("Missing language")?.clone(),
        }),
        "all" => Ok(Action::All),
        other => Err(format!("Unknown action: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_string()).collect()
    }

    #[test]
    fn env_args_rejects_short_invocation() {
        let raw = args(&["pc", "pc"]);
        let err = env_args(&raw).expect_err("short raw args should fail");

        assert!(err.contains("Not enough arguments"));
    }

    #[test]
    fn parse_action_rejects_missing_action() {
        let raw = args(&["pc", "zone", "unused"]);
        let err = parse_action_with_locales(&["all"], &raw).expect_err("missing action");

        assert!(err.contains("Missing action"));
    }

    #[test]
    fn parse_sound_zone_without_zone_is_error() {
        let raw = args(&["pc", "zone", "unused", "sound_zone", "src"]);
        let err = parse_action_with_locales(&["all"], &raw).expect_err("missing zone");

        assert_eq!(err, "Missing zone");
    }

    #[test]
    fn parse_zone_sources_partitions_locales_and_zones() {
        let raw = args(&[
            "pc",
            "zone",
            "unused",
            "zone_sources",
            "src",
            "zm_test",
            "all",
            "en",
        ]);
        let action = parse_action_with_locales(&["all", "en"], &raw).expect("parse action");

        match action {
            Action::ZoneSources {
                platform,
                languages,
                zones,
            } => {
                assert_eq!(platform, "pc");
                assert_eq!(languages, vec!["all", "en"]);
                assert_eq!(zones, vec!["zm_test"]);
            }
            _ => panic!("expected zone sources action"),
        }
    }
}
