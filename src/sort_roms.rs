use super::crud::*;
use super::model::*;
use super::progress::*;
use super::prompt::*;
use super::util::*;
use super::SimpleResult;
use clap::ArgMatches;
use diesel::SqliteConnection;
use rayon::prelude::*;
use regex::Regex;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn sort_roms(
    connection: &SqliteConnection,
    matches: &ArgMatches,
    rom_directory: &PathBuf,
) -> SimpleResult<()> {
    let progress_bar = get_progress_bar(0, get_none_progress_style());

    let systems = prompt_for_systems(&connection, matches.is_present("ALL"));

    // unordered regions to keep
    let mut regions: Vec<&str> = Vec::new();
    if matches.is_present("REGIONS") {
        regions = matches.values_of("REGIONS").unwrap().collect();
    }

    // ordered regions to use for 1G1R
    let mut one_regions: Vec<&str> = Vec::new();
    if matches.is_present("1G1R") {
        one_regions = matches.values_of("1G1R").unwrap().collect();
    }

    for system in systems {
        progress_bar.println(&format!("Processing {}", system.name));
        progress_bar.set_message("Processing games");

        let mut all_regions_games: Vec<Game> = Vec::new();
        let mut one_region_games: Vec<Game> = Vec::new();
        let mut trash_games: Vec<Game> = Vec::new();
        let mut romfile_moves: Vec<(Romfile, String)> = Vec::new();

        let mut games: Vec<Game>;

        // collect unwanted keywords
        let mut unwanted_keywords: Vec<&str> = Vec::new();

        if matches.is_present("NO-BETA") {
            unwanted_keywords.push("Beta( [0-9]+)?");
        }

        if matches.is_present("NO-DEBUG") {
            unwanted_keywords.push("Debug");
        }

        if matches.is_present("NO-DEMO") {
            unwanted_keywords.push("Demo");
        }

        if matches.is_present("NO-PROGRAM") {
            unwanted_keywords.push("Program");
        }

        if matches.is_present("NO-PROTO") {
            unwanted_keywords.push("Proto( [0-9]+)?");
        }

        if matches.is_present("NO-SAMPLE") {
            unwanted_keywords.push("Sample");
        }

        if matches.is_present("NO-SEGA-CHANNEL") {
            unwanted_keywords.push("Sega Channel");
        }

        if matches.is_present("NO-VIRTUAL-CONSOLE") {
            unwanted_keywords.push("([A-z ]+)?Virtual Console");
        }

        // compile unwanted regex
        let mut unwanted_regex: Option<Regex> = None;
        if !unwanted_keywords.is_empty() {
            unwanted_regex = Some(
                Regex::new(&format!(r"\((({})(, )?)+\)", unwanted_keywords.join("|"))).unwrap(),
            );
        }

        // 1G1R mode
        if !one_regions.is_empty() {
            let grouped_games = find_grouped_games_by_system(connection, &system);

            for (parent, mut clones) in grouped_games {
                let mut parent_clones: Vec<Game> = vec![parent];
                parent_clones.append(&mut clones);

                // trim unwanted games
                if unwanted_regex.is_some() {
                    let unwanted_regex = unwanted_regex.as_ref().unwrap();
                    let (mut unwanted_games, regular_games): (Vec<Game>, Vec<Game>) = parent_clones
                        .into_par_iter()
                        .partition(|game| unwanted_regex.find(&game.name).is_some());
                    trash_games.append(&mut unwanted_games);
                    parent_clones = regular_games;
                }

                // find the one game we want to keep, if any
                for region in &one_regions {
                    let i = parent_clones
                        .iter()
                        .position(|game| game.regions.contains(region));
                    if i.is_some() {
                        one_region_games.push(parent_clones.remove(i.unwrap()));
                        break;
                    }
                }

                // go through the remaining games
                while !parent_clones.is_empty() {
                    let game = parent_clones.remove(0);
                    if regions.iter().any(|region| game.regions.contains(region)) {
                        all_regions_games.push(game);
                    } else {
                        trash_games.push(game);
                    }
                }
            }
        // Regions mode
        } else if !regions.is_empty() {
            games = find_games_by_system(&connection, &system);

            // trim unwanted games
            if unwanted_regex.is_some() {
                let unwanted_regex = unwanted_regex.as_ref().unwrap();
                let (mut unwanted_games, regular_games): (Vec<Game>, Vec<Game>) = games
                    .into_par_iter()
                    .partition(|game| unwanted_regex.find(&game.name).is_some());
                trash_games.append(&mut unwanted_games);
                games = regular_games;
            }

            for game in games {
                if regions.iter().any(|region| game.regions.contains(region)) {
                    all_regions_games.push(game);
                } else {
                    trash_games.push(game);
                }
            }
        } else {
            games = find_games_by_system(&connection, &system);

            // trim unwanted games
            if unwanted_regex.is_some() {
                let unwanted_regex = unwanted_regex.as_ref().unwrap();
                let (mut unwanted_games, regular_games): (Vec<Game>, Vec<Game>) = games
                    .into_par_iter()
                    .partition(|game| unwanted_regex.find(&game.name).is_some());
                trash_games.append(&mut unwanted_games);
                games = regular_games;
            }

            all_regions_games.append(&mut games);
        }

        if matches.is_present("MISSING") {
            progress_bar.set_message("Processing missing games");
            let mut game_ids: Vec<i64> = Vec::new();
            game_ids.append(&mut all_regions_games.iter().map(|game| game.id).collect());
            game_ids.append(&mut one_region_games.iter().map(|game| game.id).collect());
            let missing_roms: Vec<Rom> =
                find_roms_without_romfile_by_game_ids(&connection, &game_ids);

            progress_bar.println("Missing:");
            for rom in missing_roms {
                progress_bar.println(&format!("{} [{}]", rom.name, rom.crc.to_uppercase()));
            }
        }

        // create necessary directories
        let all_regions_directory = rom_directory.join(system.name);
        let one_region_directory = all_regions_directory.join("1G1R");
        let trash_directory = all_regions_directory.join("Trash");
        for d in vec![
            &all_regions_directory,
            &one_region_directory,
            &trash_directory,
        ] {
            create_directory(&d)?;
        }

        // process all_region_games
        romfile_moves.append(&mut process_games(
            &connection,
            all_regions_games,
            &all_regions_directory,
        ));

        // process one_region_games
        romfile_moves.append(&mut process_games(
            &connection,
            one_region_games,
            &one_region_directory,
        ));

        // process trash_games
        romfile_moves.append(&mut process_games(
            &connection,
            trash_games,
            &trash_directory,
        ));

        // sort moves and print a summary
        romfile_moves.sort_by(|a, b| a.1.cmp(&b.1));
        romfile_moves.dedup_by(|a, b| a.1 == b.1);

        progress_bar.println("Summary:");
        for file_move in &romfile_moves {
            progress_bar.println(&format!("{} -> {}", file_move.0.path, file_move.1));
        }

        // prompt user for confirmation
        if prompt_for_yes_no(matches) {
            for romfile_move in romfile_moves {
                let old_path = Path::new(&romfile_move.0.path).to_path_buf();
                let new_path = Path::new(&romfile_move.1).to_path_buf();
                rename_file(&old_path, &new_path)?;
                let romfile_input = RomfileInput {
                    path: &romfile_move.1,
                };
                update_romfile(&connection, &romfile_move.0, &romfile_input);
            }
        }
    }

    Ok(())
}

fn process_games(
    connection: &SqliteConnection,
    games: Vec<Game>,
    directory: &PathBuf,
) -> Vec<(Romfile, String)> {
    let mut romfile_moves: Vec<(Romfile, String)> = Vec::new();

    let roms_romfiles = find_roms_romfiles_with_romfile_by_games(&connection, &games);
    let game_roms_romfiles: Vec<(Game, Vec<(Rom, Romfile)>)> =
        games.into_par_iter().zip(roms_romfiles).collect();

    for (game, roms_romfiles) in game_roms_romfiles {
        let rom_count = roms_romfiles.len();
        romfile_moves.append(
            &mut roms_romfiles
                .into_par_iter()
                .map(|(rom, romfile)| {
                    let new_path = String::from(
                        get_new_path(&game, &rom, &romfile, rom_count, &directory)
                            .as_os_str()
                            .to_str()
                            .unwrap(),
                    );
                    return (romfile, new_path);
                })
                .filter(|(romfile, new_path)| &romfile.path != new_path)
                .collect(),
        );
    }

    return romfile_moves;
}

fn get_new_path(
    game: &Game,
    rom: &Rom,
    romfile: &Romfile,
    rom_count: usize,
    directory: &PathBuf,
) -> PathBuf {
    let archive_extensions = vec!["7z", "zip"];
    let chd_extension = "chd";
    let cso_extension = "cso";

    let romfile_path = Path::new(&romfile.path).to_path_buf();
    let romfile_extension = romfile_path.extension().unwrap().to_str().unwrap();
    let mut new_romfile_path: PathBuf;

    if archive_extensions.contains(&romfile_extension) {
        let mut romfile_name = match rom_count {
            1 => OsString::from(&rom.name),
            _ => OsString::from(&game.name),
        };
        romfile_name.push(".");
        romfile_name.push(&romfile_extension);
        new_romfile_path = directory.join(romfile_name);
    } else if romfile_extension == chd_extension {
        if rom_count == 2 {
            new_romfile_path = directory.join(&rom.name);
            new_romfile_path.set_extension(&romfile_extension);
        } else {
            let mut romfile_name = OsString::from(&game.name);
            romfile_name.push(".");
            romfile_name.push(&romfile_extension);
            new_romfile_path = directory.join(romfile_name);
        }
    } else if romfile_extension == cso_extension {
        new_romfile_path = directory.join(&rom.name);
        new_romfile_path.set_extension(&romfile_extension);
    } else {
        new_romfile_path = directory.join(&rom.name);
    }
    new_romfile_path
}
