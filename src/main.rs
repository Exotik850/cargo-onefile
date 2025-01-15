use anyhow::{bail, Result};
use args::{Commands, OnefileArgs};
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use ignore::{WalkBuilder, WalkState};
use rayon::prelude::*;
use std::io::{BufRead, BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
mod args;
mod metadata;
use metadata::ProjectMetadata;

fn main() -> Result<()> {
    let Commands::Onefile(args) = Commands::parse();
    let args = Arc::new(args.clone());

    verify_args(&args)?;

    let start = args.info.then(Instant::now);

    let metadata = if args.include_metadata {
        Some(ProjectMetadata::from_manifest(&args.manifest_path)?)
    } else {
        None
    };

    let source_files = collect_source_files(&args)?;

    if source_files.is_empty() {
        eprintln!("No files found to include");
        return Ok(());
    }

    generate_output(&args, source_files, metadata, start)
}

fn print_info_summary(file_contents: Vec<(PathBuf, Vec<u8>)>, start: Instant) {
    let elapsed = start.elapsed();
    let sum = file_contents
        .iter()
        .map(|(_, content)| content.lines().count())
        .sum::<usize>();

    eprintln!(
        "Found {} files\nTotal Lines of Code: {sum}\nTime Elapsed: {}.{:03}s",
        file_contents.len(),
        elapsed.as_secs(),
        elapsed.subsec_millis()
    );
}

fn generate_table_of_contents(file_contents: &[(PathBuf, Vec<u8>)], head_len: usize) -> String {
    assert!(file_contents.len() > 0, "No files to generate table of contents");
    let toc_len = file_contents.len() + 5 + head_len;
    let mut curr_line = 0;
    let mut toc = String::from("// Table of Contents\n// ==================\n");
    for (file, content) in file_contents {
        let disp = file.display().to_string();
        toc.push_str(&format!(
            "// Ln{} : {}\n",
            curr_line + toc_len,
            disp.trim_start_matches("\\\\?\\")
        ));
        curr_line += content.lines().count() + 2;
    }
    toc + "// ==================\n"
}

fn generate_output(
    args: &OnefileArgs,
    file_contents: Vec<(PathBuf, Vec<u8>)>,
    metadata: Option<ProjectMetadata>,
    start: Option<Instant>,
) -> Result<()> {
    let head = args.head.as_ref().map(std::fs::read).transpose()?;
    let table_of_contents = args.table_of_contents.then(|| {
        generate_table_of_contents(&file_contents, head.map_or(0, |h| h.len())).into_bytes()
    });

    if let Some(start) = start {
        print_info_summary(file_contents, start);
        return Ok(());
    }

    let cursor = if args.stdout {
        &mut BufWriter::new(std::io::stdout()) as &mut dyn Write
    } else {
        &mut BufWriter::new(std::fs::File::create(&args.output)?) as &mut dyn Write
    };

    write_output(cursor, args, file_contents, metadata, table_of_contents)?;

    Ok(())
}

fn write_output(
    cursor: &mut dyn Write,
    args: &OnefileArgs,
    file_contents: Vec<(PathBuf, Vec<u8>)>,
    metadata: Option<ProjectMetadata>,
    table_of_contents: Option<Vec<u8>>,
) -> Result<()> {
    if let Some(head) = &args.head {
        let head_content = std::fs::read(head)?;
        cursor.write(&head_content)?;
    }

    if let Some(metadata) = metadata {
        let meta = metadata.format();
        cursor.write(meta.as_bytes())?;
    }

    if let Some(toc) = table_of_contents {
        cursor.write(&toc)?;
    }

    for (path, contents) in file_contents {
        writeln!(cursor, "{} {}", &args.separator, path.display())?;
        cursor.write(&contents)?;
        cursor.write(&[b'\n'])?;
    }

    Ok(())
}

fn verify_args(args: &OnefileArgs) -> Result<()> {
    if let (Some(st), Some(lt)) = (&args.smaller_than, &args.larger_than) {
        if st > lt {
            bail!("`smaller_than` cannot be larger than `larger_than`");
        }
    }

    if let (Some(nt), Some(ot)) = (&args.newer_than, &args.older_than) {
        if nt > ot {
            bail!("`newer_than` cannot be older than `older_than`");
        }
    }
    Ok(())
}

fn filter_path(
    extension: &Vec<String>,
    smaller_than: &Option<u64>,
    larger_than: &Option<u64>,
    newer_than: &Option<NaiveDateTime>,
    older_than: &Option<NaiveDateTime>,
    include_lock: bool,
    f: ignore::DirEntry,
) -> Option<PathBuf> {
    let path = f.path();

    if !include_lock && path.as_os_str().to_str() == Some("Cargo.lock") {
        return None;
    };

    // Extension filter
    if !extension.iter().any(|ext_user| {
        path.extension()
            .map_or(false, |ext_file| ext_file.to_str() == Some(ext_user))
    }) {
        return None;
    }

    // Size and date filters
    if smaller_than.is_some() || larger_than.is_some() {
        let metadata = f.metadata().ok()?;
        let meta_len = metadata.len();
        if smaller_than.is_some_and(|st| meta_len > st) {
            return None;
        }
        if larger_than.is_some_and(|lt| meta_len < lt) {
            return None;
        }
    }

    if older_than.is_some() || newer_than.is_some() {
        let metadata = f.metadata().ok()?;
        let modified: DateTime<Utc> = metadata.modified().ok()?.into();
        if older_than.is_some_and(|ot| modified > ot.and_utc()) {
            return None;
        }
        if newer_than.is_some_and(|nt| modified < nt.and_utc()) {
            return None;
        }
    }

    Some(path.to_path_buf())
}

fn collect_source_files(args: &OnefileArgs) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let Some(manifest_parent) = args.manifest_path.parent() else {
        // If the manifest path has no parent, we can't search for other files
        bail!(
            "Cargo.toml has no parent directory: {}",
            args.manifest_path.display()
        );
    };
    let mut search_paths = args
        .include
        .iter()
        .filter(|&f| {
            let x = f.is_dir() || f.is_file();
            if !x {
                eprintln!("File not found: {}", f.display());
            }
            x
        })
        .cloned()
        .collect::<Vec<_>>();

    // if !manifest_path.exists() {
    //     bail!("Cargo.toml not found at {}", manifest_path.display());
    // }

    let manifest = cargo_toml::Manifest::from_path(&args.manifest_path)?;
    search_paths.extend(manifest.workspace.into_iter().flat_map(|workspace| {
        workspace
            .members
            .into_iter()
            .map(|f| manifest_parent.join(f))
    }));
    search_paths.push(manifest_parent.to_owned());

    if args.dependencies {
        let deps = manifest
            .dependencies
            .into_iter()
            .filter_map(|(_, dep)| {
                // let path = dep.path.unwrap_or_else(|| format!("../{}", name));
                dep.detail()
                    .and_then(|f| f.path.as_ref())
                    .map(|f| manifest_parent.join(f))
            })
            .collect::<Vec<_>>();

        search_paths.extend(deps);
    }

    let mut walker = WalkBuilder::new(search_paths[0].clone());
    for path in search_paths.iter().skip(1) {
        walker.add(path);
    }

    setup_walker(&mut walker, args);

    // for exclude in &args.exclude {
    //     walker.add_ignore(exclude);
    // }

    let (tx, rx) = std::sync::mpsc::channel();
    walker
        // .standard_filters(args.skip_gitignore)
        // .max_depth(args.depth)
        .build_parallel()
        .run(|| {
            let tx = tx.clone();
            let args = args.clone();
            Box::new(move |result| {
                let Ok(path) = result else {
                    println!("Error: {:?}", result.unwrap_err());
                    return WalkState::Continue;
                };

                if let Some(path) = filter_path(
                    &args.extension,
                    &args.smaller_than,
                    &args.larger_than,
                    &args.newer_than,
                    &args.older_than,
                    args.include_lock,
                    path,
                ) {
                    tx.send(path).unwrap();
                }
                WalkState::Continue
            })
        });
    drop(tx);
    let mut source_files = rx.iter().collect::<Vec<_>>();

    if source_files.is_empty() {
        bail!("No files found to include");
    }

    // If there are any directories, get the files from them
    reduce_dir_list(&mut source_files, args)?;

    if let Some(max_files) = args.max_files {
        if source_files.len() > max_files {
            eprintln!(
                "Found {} files, but the maximum number of files is set to {}, truncating to fit the desired amount of files",
                source_files.len(),
                max_files
            );
            source_files.truncate(max_files);
        }
    }

    // For each path, read the contents into a string
    let mut file_contents: Vec<_> = source_files
        .par_iter()
        // .filter(|f| f.is_file())
        .filter_map(|file| match std::fs::read(file) {
            Ok(content) => Some((file.clone(), content)),
            Err(e) => {
                eprintln!("Error reading file {}: {e}", file.display());
                None
            }
        })
        .collect();

    // Sort the files by path
    file_contents.par_sort_by_cached_key(|(a, _)| a.clone());
    Ok(file_contents)
}

fn setup_walker(walker: &mut WalkBuilder, args: &OnefileArgs) {
    for excl in &args.exclude {
        walker.add(excl);
    }
    walker
        .max_depth(args.depth)
        .standard_filters(args.skip_gitignore);
}

/// Reduces a list of paths to files and/or dirs to a list of dirs to only files.
/// This function avoids iterating over the entire list multiple times by using a single pass
/// to collect directories and then processing them in bulk.
fn reduce_dir_list(paths: &mut Vec<PathBuf>, args: &OnefileArgs) -> Result<()> {
    // Collect indices of directories in the list
    let dir_indices: Vec<_> = paths
        .iter()
        .enumerate()
        .filter_map(|(i, path)| if path.is_dir() { Some(i) } else { None })
        .collect();

    if dir_indices.is_empty() {
        return Ok(());
    }

    // Remove directories from the list and collect them
    let mut dirs = dir_indices
        .into_iter()
        .rev()
        .map(|i| paths.swap_remove(i));

    // Initialize the walker with the first directory
    let mut walker = WalkBuilder::new(dirs.next().unwrap());
    for dir in dirs {
        walker.add(dir);
    }

    setup_walker(&mut walker, args);

    let new_paths = walker.build().filter_map(|result| {
        let path = result.ok()?;
        let path = path.path();
        if path.is_file() {
            Some(path.to_path_buf())
        } else {
            None
        }
    });

    // Append the new files to the original list
    paths.extend(new_paths);

    Ok(())
}