use anyhow::{bail, Result};
use clap::Parser;
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser, Debug)]
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
    #[arg(short, long, action)]
    info: bool,

    /// Add the dependencies of the project to the output.
    /// WARNING: This will increase the size of the output significantly.
    #[arg(short, long, action)]
    dependencies: bool,

    /// The separator shown between files.
    ///
    /// Example:
    ///   cargo onefile --separator "// File: "
    #[arg(long, default_value = "//")]
    separator: String,

    /// Exclude the specified files from the output.
    /// Accepts multiple values.
    ///
    /// Example:
    ///   cargo onefile --exclude "file1.rs" --exclude "file2.rs"
    #[arg(long)]
    exclude: Vec<String>,
}

fn main() -> Result<()> {
    let OnefileArgs {
        stdout,
        dependencies,
        output,
        manifest_path,
        head,
        exclude,
        depth,
        skip_gitignore,
        separator,
        info,
    } = OnefileArgs::parse();

    let start = info.then(Instant::now);

    let mut search_paths = vec![];
    let Some(manifest_parent) = manifest_path.parent() else {
        // If the manifest path has no parent, we can't search for other files
        bail!(
            "Cargo.toml has no parent directory: {}",
            manifest_path.display()
        );
    };

    if !manifest_path.exists() {
        bail!("Cargo.toml not found at {}", manifest_path.display());
    }

    let manifest = std::fs::read_to_string(&manifest_path)?;
    let manifest = cargo_toml::Manifest::from_str(&manifest)?;

    search_paths.extend(
        manifest
            .workspace
            .map(|workspace| {
                workspace
                    .members
                    .into_iter()
                    .map(|f| manifest_parent.join(f))
                    .collect()
            })
            .unwrap_or_else(|| vec![manifest_path.parent().unwrap().to_path_buf()]),
    );

    if dependencies {
        let deps = manifest
            .dependencies
            .into_iter()
            .filter_map(|(_, dep)| {
                // let path = dep.path.unwrap_or_else(|| format!("../{}", name));
                dep.detail()
                    .map(|f| f.path.as_ref())
                    .flatten()
                    .map(|f| manifest_parent.join(f))
            })
            .collect::<Vec<_>>();

        search_paths.extend(deps);
    }

    if search_paths.is_empty() {
        search_paths.push(PathBuf::from("."));
    }

    let source_files: Vec<_> = search_paths
        .into_par_iter()
        .flat_map(|f| {
            WalkBuilder::new(f)
                .standard_filters(skip_gitignore)
                .build()
                .filter_map(Result::ok)
                .take_while(|e| e.depth() <= depth)
                .filter_map(|f| {
                    let path = f.path();

                    if exclude.iter().any(|e| path.ends_with(e)) {
                        return None;
                    }

                    path.extension()
                        .map_or(false, |ext| ext == "rs")
                        .then(|| path.to_path_buf())
                })
                .collect::<Vec<_>>()
        })
        .collect();

    // For each path, read the contents into a string
    let mut file_contents: Vec<_> = source_files
        .par_iter()
        .map(|file| {
            let content = std::fs::read_to_string(file);
            (file.clone(), content)
        })
        .collect();

    // Sort the files by path
    file_contents.par_sort_by_key(|(a, _)| a.clone());

    let head = head
        .map(|head| std::fs::read_to_string(&head))
        .transpose()?;

    if let Some(start) = start {
        let elapsed = start.elapsed();
        let sum = file_contents
            .iter()
            .map(|(_, content)| content.as_ref().map_or(0, |c| c.lines().count()))
            .sum::<usize>();

        eprintln!(
            "Found {} files\nTotal Lines of Code: {sum}\nTime Elapsed: {}.{:03}s",
            file_contents.len(),
            elapsed.as_secs(),
            elapsed.subsec_millis()
        );
    } else if stdout {
        if let Some(head) = head {
            println!("{}", head);
        }
        for (file, content) in file_contents {
            println!("{} {}", separator, file.display());
            println!("{}", content?);
        }
    } else {
        let mut output = std::fs::File::create(&output)?;
        if let Some(head) = head {
            writeln!(output, "{}", head)?;
        }
        for (file, content) in file_contents {
            writeln!(output, "{} {}\n{}", separator, file.display(), content?)?;
        }
    }

    Ok(())
}
