use anyhow::{bail, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use clap::Parser;
use ignore::{WalkBuilder, WalkState};
use rayon::prelude::*;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
#[command(styles = clap_cargo::style::CLAP_STYLING)]
enum Commands {
    #[command(name = "onefile")]
    #[command(author, version, about)]
    Onefile(OnefileArgs),
}

#[derive(Parser, Debug, Clone)]
#[command(name = "Cargo Onefile")]
#[command(
    about = "Generate a single file that contains all the source code of a Rust project.
Mainly intended to pipe source code into an LLM."
)]
#[command(version, long_about=None)]
struct OnefileArgs {
    /// Output to stdout instead of a file.
    /// If this flag is set, the `output` option is ignored.
    ///
    /// Example:
    ///   cargo onefile --stdout
    #[arg(long)]
    stdout: bool,

    /// Include a table of contents at the top of the output.
    /// This will list all the files included in the output.
    ///
    /// Example:
    ///  cargo onefile --table-of-contents
    #[arg(long, action)]
    table_of_contents: bool,

    /// Optional path to the output file.
    ///
    /// Example:
    ///   cargo onefile -o ./output/combined.rs
    #[arg(short, long, default_value = "./onefile.rs")]
    output: PathBuf,

    /// Optional path to a `Cargo.toml` file.
    /// If not provided, the command will look for a `Cargo.toml` file in the current directory.
    ///
    /// Example:
    ///   cargo onefile -p ./path/to/Cargo.toml
    #[arg(short = 'p', long, default_value = "./Cargo.toml")]
    manifest_path: PathBuf,

    /// Optional path to a header file.
    /// The contents of this file will be prepended to the output.
    ///
    /// Example:
    ///   cargo onefile --head ./header.txt
    #[arg(long)]
    head: Option<PathBuf>,

    /// Maximum depth to search for files.
    ///
    /// Example:
    ///   cargo onefile --depth 5
    #[arg(long, default_value = "10")]
    depth: usize,

    /// Skip gitignored files.
    /// Enabled by default.
    ///
    /// Example:
    ///   cargo onefile --skip-gitignore false
    #[arg(long, default_value = "true")]
    skip_gitignore: bool,

    /// Info mode.
    /// This flag is used to measure the performance of the command, as well as the number of files found and the number of lines of code.
    /// It will not write to a file or stdout.
    #[arg(short = 'I', long, action)]
    info: bool,

    /// Add the dependencies of the project to the output.
    ///
    /// WARNING: This will increase the size of the output significantly.
    #[arg(short, long, action)]
    dependencies: bool,

    /// The separator shown between files.
    ///
    /// Example:
    ///   cargo onefile --separator "// File: "
    #[arg(long, default_value = "//")]
    separator: String,

    /// Exclude files older than the specified datetime.
    ///
    /// Format: "YYYY-MM-DD HH:MM:SS"
    ///
    /// Will not work if `newer_than` is also set and is older than `older_than`.
    ///
    /// Example:
    ///  cargo onefile --older-than "2021-01-01 00:00:00"
    #[arg(long)]
    newer_than: Option<NaiveDateTime>,
    /// Exclude files newer than the specified datetime.
    ///
    /// Format: "YYYY-MM-DD HH:MM:SS"
    ///
    /// Will not work if `older_than` is also set and is newer than `newer_than`.
    ///
    /// Example:
    ///   cargo onefile --newer-than "2021-01-01 00:00:00"
    #[arg(long)]
    older_than: Option<NaiveDateTime>,

    /// Exclude files larger than the specified size in bytes.
    ///
    /// Will not work if `smaller_than` is also set and is larger than `larger_than`.
    ///
    /// Example:
    ///  cargo onefile --larger-than 1000000
    #[arg(long)]
    smaller_than: Option<u64>,

    /// Exclude files smaller than the specified size in bytes.
    ///
    /// Will not work if `larger_than` is also set and is smaller than `smaller_than`.
    ///
    /// Example:
    ///   cargo onefile --smaller-than 1000
    #[arg(long)]
    larger_than: Option<u64>,

    /// Max number of files to include in the output.
    /// If the number of files found exceeds this value, the command will ignore the rest of the files found past this number.
    ///
    /// Example:
    ///  cargo onefile --max-files 100
    #[arg(long)]
    max_files: Option<usize>,

    /// Add a path to include in the output
    ///
    /// If the path is a directory, all files in the directory will be included.
    ///
    /// Example:
    /// cargo onefile --include "file1.rs" --include "util/components"
    #[arg(short, long)]
    include: Vec<PathBuf>,

    /// Include files with the specified extension.
    /// Defaults to "rs".
    ///
    /// Example:
    ///  cargo onefile --extension toml
    #[arg(short = 'E', long)]
    extension: Option<Vec<String>>,

    /// Exclude the specified files from the output.
    /// Accepts multiple values.
    ///
    /// Example:
    ///   cargo onefile --exclude "file1.rs" --exclude "file2.rs"
    #[arg(short, long)]
    exclude: Vec<String>,
}

fn main() -> Result<()> {
    let Commands::Onefile(args) = Commands::parse();
    let OnefileArgs {
        stdout,
        dependencies,
        output,
        manifest_path,
        head,
        separator,
        info,
        newer_than,
        older_than,
        smaller_than,
        larger_than,
        table_of_contents,
        max_files,
        include,
        
        
        ..
    } = args.clone();

    if let (Some(st), Some(lt)) = (smaller_than, larger_than) {
        if st > lt {
            bail!("`smaller_than` cannot be larger than `larger_than`");
        }
    }

    if let (Some(nt), Some(ot)) = (newer_than, older_than) {
        if nt > ot {
            bail!("`newer_than` cannot be older than `older_than`");
        }
    }

    let start = info.then(Instant::now);

    let mut search_paths = include
        .into_iter()
        .filter(|f| {
            let x = f.is_dir() || f.is_file();
            if !x {
                eprintln!("File not found: {}", f.display());
            }
            x
        })
        .collect::<Vec<_>>();
    let Some(manifest_parent) = manifest_path.parent() else {
        // If the manifest path has no parent, we can't search for other files
        bail!(
            "Cargo.toml has no parent directory: {}",
            manifest_path.display()
        );
    };

    // if !manifest_path.exists() {
    //     bail!("Cargo.toml not found at {}", manifest_path.display());
    // }

    let manifest = cargo_toml::Manifest::from_path(&manifest_path)?;
    search_paths.extend(manifest.workspace.into_iter().flat_map(|workspace| {
        workspace
            .members
            .into_iter()
            .map(|f| manifest_parent.join(f))
    }));
    search_paths.push(manifest_parent.to_owned());

    if dependencies {
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

    if search_paths.is_empty() {
        eprintln!("No files found to include, searching in current directory");
        search_paths.push(PathBuf::from("."));
    }

    let args = Arc::new(args);
    // let mut source_files: Vec<_> = search_paths
    //     .into_par_iter()
    //     .flat_map_iter(|f| walk_path(f, args.clone()))
    //     .collect();
    let mut source_files = WalkBuilder::new(search_paths[0].clone());
    for path in search_paths.iter().skip(1) {
        source_files.add(path);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    source_files
        .standard_filters(args.skip_gitignore)
        .build_parallel()
        .run(|| {
            let tx = tx.clone();
            let args = args.clone();
            Box::new(move |result| {
                let Ok(path) = result else {
                    return WalkState::Continue;
                };

                if let Some(path) = filter_path(
                    &args.exclude,
                    &args.extension,
                    &args.smaller_than,
                    &args.larger_than,
                    &args.newer_than,
                    &args.older_than,
                    path,
                ) {
                    tx.send(path).unwrap();
                }
                WalkState::Continue
            })
        });
    drop(tx);
    let mut source_files = rx.iter().collect::<Vec<_>>();
    // .filter_map(Result::ok)
    // .flat_map(|f| {
    //     filter_path(
    //         &exclude,
    //         &extension,
    //         &smaller_than,
    //         &larger_than,
    //         &newer_than,
    //         &older_than,
    //         f,
    //     )
    // })
    // .collect::<Vec<_>>();

    if source_files.is_empty() {
        eprintln!("No files found to include");
        return Ok(());
    }

    if let Some(max_files) = max_files {
        if source_files.len() > max_files {
            eprintln!(
                "Found {} files, but the maximum number of files is set to {}",
                source_files.len(),
                max_files
            );
            source_files.truncate(max_files);
        }
    }

    // For each path, read the contents into a string
    let mut file_contents: Vec<_> = source_files
        .par_iter()
        .filter_map(|file| match std::fs::read_to_string(file) {
            Ok(content) => Some((file.clone(), content)),
            Err(e) => {
                eprintln!("Error reading file: {e}");
                None
            }
        })
        .collect();

    // Sort the files by path
    file_contents.par_sort_by_cached_key(|(a, _)| a.clone());

    let head = head
        .map(|head| std::fs::read_to_string(&head))
        .transpose()?;

    let table_of_contents = table_of_contents.then(|| {
        let toc_len = file_contents.len() + 5 + head.as_ref().map_or(0, String::len);
        let mut curr_line = 0;
        let mut toc = String::from("// Table of Contents\n// ==================\n");
        for (file, content) in &file_contents {
            let display = std::fs::canonicalize(file)
                .as_ref()
                .unwrap_or(file)
                .display()
                .to_string();
            toc.push_str(&format!(
                "// Ln{} : {}\n",
                curr_line + toc_len,
                display.trim_start_matches("\\\\?\\")
            ));
            curr_line += content.lines().count() + 2;
        }
        toc + "// ==================\n"
    });

    if let Some(start) = start {
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
    } else if stdout {
        if let Some(head) = head {
            println!("{head}");
        }
        if let Some(toc) = table_of_contents {
            println!("{toc}");
        }
        for (file, content) in file_contents {
            println!("{} {}", separator, file.display());
            println!("{content}");
        }
    } else {
        let mut output = std::fs::File::create(&output)?;
        if let Some(head) = head {
            writeln!(output, "{head}")?;
        }
        if let Some(toc) = table_of_contents {
            writeln!(output, "{toc}")?;
        }
        for (file, content) in file_contents {
            writeln!(output, "{} {}\n{}", separator, file.display(), content)?;
        }
    }

    Ok(())
}

fn walk_path(path: impl AsRef<std::path::Path>, args: Arc<OnefileArgs>) -> Vec<PathBuf> {
    if path.as_ref().is_file() {
        return vec![path.as_ref().to_owned()];
    }
    let OnefileArgs {
        depth,
        skip_gitignore,
        exclude,
        extension,
        smaller_than,
        larger_than,
        newer_than,
        older_than,
        ..
    } = args.as_ref();
    WalkBuilder::new(path)
        .standard_filters(*skip_gitignore)
        .build()
        // .run(|| {})
        .filter_map(Result::ok)
        .take_while(|e| e.depth() <= *depth)
        .filter_map(|f| {
            filter_path(
                exclude,
                extension,
                smaller_than,
                larger_than,
                newer_than,
                older_than,
                f,
            )
        })
        .collect()
}

fn filter_path(
    exclude: &Vec<String>,
    extension: &Option<Vec<String>>,
    smaller_than: &Option<u64>,
    larger_than: &Option<u64>,
    newer_than: &Option<NaiveDateTime>,
    older_than: &Option<NaiveDateTime>,
    f: ignore::DirEntry,
) -> Option<PathBuf> {
    let path = f.path();

    // Extension filter
    if let Some(extension) = &extension {
        if !extension.iter().any(|ext_user| {
            path.extension()
                .map_or(false, |ext_file| ext_file.to_str() == Some(ext_user))
        }) {
            return None;
        }
    } else if path.extension().map_or(false, |ext| ext == "rs") {
        return None;
    }

    // Exclude filter
    if exclude.iter().any(|e| path.ends_with(e)) {
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
