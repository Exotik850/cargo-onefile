use anyhow::{Context, Result};
use std::path::Path;

/// Represents project metadata that will be included at the top of the generated file
#[derive(Debug)]
pub struct ProjectMetadata {
    name: String,
    version: String,
    description: Option<String>,
    readme: Option<String>,
    repository: Option<String>,
    authors: Vec<String>,
    license: Option<String>,
}

impl ProjectMetadata {
    pub fn from_manifest(manifest_path: &Path) -> Result<Self> {
        let manifest =
            cargo_toml::Manifest::from_path(manifest_path).context("Failed to read Cargo.toml")?;

        let package = manifest
            .package
            .context("No package section found in Cargo.toml")?;
        let description = package.description().map(|s| s.to_string());
        let repository = package.repository().map(|s| s.to_string());
        let authors = package.authors().into_iter().cloned().collect();
        let license = package.license().map(|s| s.to_string());
        let version = package.version().to_string();

        // Try to read README file
        // let readme_content = if let Some(readme_path) = package.readme. {
        //     let readme_path = manifest_path.parent().unwrap().join(readme_path);
        //     std::fs::read_to_string(readme_path).ok()
        // } else {
        //     // Try common README filenames if not specified
        //     let parent = manifest_path.parent().unwrap();
        //     ["README.md", "README", "Readme.md"]
        //         .iter()
        //         .find_map(|name| std::fs::read_to_string(parent.join(name)).ok())
        // };
        let readme = package
            .readme()
            .is_some()
            .then(|| {
                let path = package.readme().as_path()?;
                let path = manifest_path.parent()?.join(path);
                std::fs::read_to_string(path).ok()
            })
            .flatten();

        Ok(Self {
            name: package.name,
            version,
            description,
            readme,
            repository,
            authors,
            license,
        })
    }

    pub fn format(&self) -> String {
        let mut output = String::new();

        // Project header
        output.push_str(&format!("// Project: {} (v{})\n", self.name, self.version));

        if let Some(desc) = &self.description {
            output.push_str(&format!("// Description: {}\n", desc));
        }

        if !self.authors.is_empty() {
            output.push_str(&format!("// Authors: {}\n", self.authors.join(", ")));
        }

        if let Some(license) = &self.license {
            output.push_str(&format!("// License: {}\n", license));
        }

        if let Some(repo) = &self.repository {
            output.push_str(&format!("// Repository: {}\n", repo));
        }

        output.push_str("\n");

        // Add README content if available
        if let Some(readme) = &self.readme {
            output.push_str("// README\n");
            output.push_str("// ======\n");
            for line in readme.lines() {
                output.push_str(&format!("// {}\n", line));
            }
            output.push_str("// ======\n\n");
        }

        output
    }
}
