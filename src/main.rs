use clap::Parser;

mod ambient_bsp;
mod asset_types;
mod bank;
mod checksum;
mod converter;
mod decoded_audio;
mod duk;
mod env;
mod filespec;
mod flac;
mod music;
mod obtainer;
mod ogg;
mod registry;
mod riff;
mod self_update;
mod sound_data_snapshot;
mod sound_zone;
mod sound_zone_config;
mod source_validation;
mod staged_outputs;
mod stock_donor;
mod string_hash;
mod sz_writer;
mod tables;
mod units;
mod update_bank;

#[cfg(test)]
mod regression_tests;

use env::Env;
use sound_data_snapshot::SoundDataSnapshot;
use sound_zone::SoundZone;
use sound_zone_config::SoundZoneConfig;
use update_bank::update_bank;

const RAW_ARG_USAGE: &str = "Expected raw args: <platform> <platform_working_dir> <unused> <action> <source_dir> [action args...]";

/// The Black Ops III Mod Tools' sound converter, rebuilt.
///
/// The Linker runs it as `sound\snd_convert.exe`; you don't usually run it yourself.
#[derive(Parser)]
#[command(name = "Ultrasound", version)]
struct Args {
    /// Also report how many sources were confirmed unchanged without being read
    #[arg(long)]
    verbose: bool,

    /// Accepted for compatibility with the Mod Tools; has no effect
    #[arg(long)]
    skip_deps: bool,

    /// Internal: check for and install an update, started in the background
    #[arg(long, hide = true)]
    update: bool,

    /// Print what this build can do (features, stock sound request versions) as one JSON line, and exit
    #[arg(long)]
    capabilities: bool,

    /// Ask the release feed for a newer Ultrasound and print the answer; installs nothing
    #[arg(long)]
    check_update: bool,

    /// With --check-update: print one JSON line
    #[arg(long, requires = "check_update")]
    json: bool,

    /// With --check-update: skip the daily cache
    #[arg(long, requires = "check_update")]
    refresh: bool,

    /// The arguments the Linker passes: <platform> <platform_working_dir> <unused> <action>
    /// <source_dir> [action args...]
    raw_args: Vec<String>,
}

/// Set by `--verbose`.
pub static VERBOSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
    if args.update {
        self_update::run_update();
        return Ok(());
    }
    if args.check_update {
        self_update::check_update(args.json, args.refresh);
        return Ok(());
    }
    // (asked by Dragon before it carries stock sounds: nothing else is printed, read or updated)
    if args.capabilities {
        println!("{}", registry::capabilities_json());
        return Ok(());
    }
    let started = std::time::Instant::now();
    VERBOSE.store(args.verbose, std::sync::atomic::Ordering::Relaxed);
    println!("Ultrasound {}", env!("CARGO_PKG_VERSION"));
    self_update::on_start();
    registry::record_self();

    let (platform_working_dir, source_dir) = env_args(&args.raw_args)?;

    let env = Env::new(platform_working_dir, source_dir)?;
    let mut snapshot = SoundDataSnapshot::new(env)?;

    let action = parse_action(&snapshot, &args.raw_args)?;

    run(&mut snapshot, action)?;
    println!("Done in {:.1} s", started.elapsed().as_secs_f64());
    self_update::after_success();
    Ok(())
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

    let mut staged = staged_outputs::StagedOutputs::new(snapshot.env.get_platform_working_dir())?;
    let output_env = snapshot.env.with_output_root(staged.root());
    let locale = snapshot
        .get_locale(language)
        .cloned()
        .ok_or_else(|| format!("unknown language '{}'", language))?;
    zone.write_outputs(&output_env, &config, &locale)?;
    let donors = stock_donor::load(
        snapshot.env.get_game_dir(),
        snapshot.env.get_source_dir(),
        &zone.name,
        language,
        &locale.deploy_name,
    )?;

    // Build both banks concurrently — they share the same rayon pool for
    // FLAC encoding, so the win is mostly from eliminating the gap between
    // the two sequential runs rather than 2x throughput.
    let snap: &SoundDataSnapshot = snapshot;
    let (r_loaded, r_streamed) = rayon::join(
        || update_bank(snap, &output_env, &zone, platform, language, false, &donors),
        || update_bank(snap, &output_env, &zone, platform, language, true, &donors),
    );
    r_loaded?;
    r_streamed?;
    staged.publish(snapshot.env.get_platform_working_dir())?;
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
    fn capabilities_flag_needs_no_linker_args() {
        let a = Args::try_parse_from(["snd_convert.exe", "--capabilities"]).expect("parse");
        assert!(a.capabilities);
        assert!(a.raw_args.is_empty());
        let a = Args::try_parse_from([
            "snd_convert.exe",
            "pc",
            "usermaps\\m",
            "usermaps\\m",
            "zone_source",
            "usermaps\\m",
            "all",
            "m",
        ])
        .expect("parse");
        assert!(!a.capabilities);
    }

    #[test]
    fn check_update_flags_parse_without_linker_args() {
        let a =
            Args::try_parse_from(["snd_convert.exe", "--check-update", "--json"]).expect("parse");
        assert!(a.check_update && a.json && !a.refresh);
        assert!(Args::try_parse_from(["snd_convert.exe", "--json"]).is_err());
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
