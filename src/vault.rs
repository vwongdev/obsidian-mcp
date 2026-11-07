use std::{
    fmt, io,
    io::Write,
    path::{Path, PathBuf},
};

use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};

const CONFIG_DIR: &str = ".obsidian";

/// A vault directory handle; every path is opened relative to it, so `..` and symlinks cannot escape.
pub struct Vault {
    root: Dir,
    write_folder: PathBuf,
}

#[derive(Debug)]
pub enum VaultError {
    InvalidPath(String),
    Io(io::Error),
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPath(reason) => write!(f, "invalid path: {reason}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for VaultError {}

impl From<io::Error> for VaultError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, VaultError>;

impl Vault {
    /// Opens a vault that only permits writes inside `write_folder`.
    pub fn open(path: impl AsRef<Path>, write_folder: &str) -> Result<Self> {
        let mut vault = Self {
            root: Dir::open_ambient_dir(path, ambient_authority())?,
            write_folder: PathBuf::new(),
        };
        vault.write_folder = vault.resolve(write_folder)?;
        Ok(vault)
    }

    pub fn write_folder(&self) -> &Path {
        &self.write_folder
    }

    pub fn read(&self, path: &str) -> Result<String> {
        Ok(self.root.read_to_string(self.resolve(path)?)?)
    }

    /// Writes a new file, failing if one already exists.
    pub fn create(&self, path: &str, content: &str) -> Result<()> {
        let path = self.resolve_writable(path)?;
        self.create_parent(&path)?;
        let mut file = self
            .root
            .open_with(&path, OpenOptions::new().write(true).create_new(true))?;
        Ok(file.write_all(content.as_bytes())?)
    }

    /// Appends to an existing file.
    pub fn append(&self, path: &str, content: &str) -> Result<()> {
        let path = self.resolve_writable(path)?;
        let mut file = self
            .root
            .open_with(&path, OpenOptions::new().append(true))?;
        Ok(file.write_all(content.as_bytes())?)
    }

    /// Writes a file in full, renaming a temporary file into place so readers never see partial content.
    pub fn replace(&self, path: &str, content: &str) -> Result<()> {
        let path = self.resolve_writable(path)?;
        self.create_parent(&path)?;
        let name = path.file_name().expect("resolved paths end in a name");
        let temp = path.with_file_name(format!(".{}.tmp", name.to_string_lossy()));
        self.root.write(&temp, content)?;
        Ok(self.root.rename(&temp, &self.root, &path)?)
    }

    /// Lists files under a folder recursively, skipping symlinks and dot folders such as the Obsidian config directory.
    pub fn files(&self, folder: &str) -> Result<Vec<String>> {
        let mut files = Vec::new();
        if folder.is_empty() {
            walk(&self.root, Path::new(""), &mut files)?;
        } else {
            let folder = self.resolve(folder)?;
            walk(&self.root.open_dir(&folder)?, &folder, &mut files)?;
        }
        files.sort();
        Ok(files)
    }

    fn create_parent(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            self.root.create_dir_all(parent)?;
        }
        Ok(())
    }

    fn resolve_writable(&self, path: &str) -> Result<PathBuf> {
        let resolved = self.resolve(path)?;
        if resolved == self.write_folder || !resolved.starts_with(&self.write_folder) {
            return Err(VaultError::InvalidPath(format!(
                "{path}: writes are only allowed inside {}/",
                self.write_folder.display()
            )));
        }
        Ok(resolved)
    }

    /// Validates a vault-relative path: no `..`, no config directory, no symlinked components.
    pub fn resolve(&self, path: &str) -> Result<PathBuf> {
        let invalid = |reason: &str| Err(VaultError::InvalidPath(format!("{path}: {reason}")));
        if path.starts_with('/') {
            return invalid("must be relative to the vault");
        }
        let mut resolved = PathBuf::new();
        for component in path.split('/') {
            match component {
                "" | "." | ".." => return invalid("must not contain empty, '.' or '..' segments"),
                c if c.contains('\\') => return invalid("must use '/' as separator"),
                c if is_config_dir(c) => {
                    return invalid("the Obsidian config directory is off limits");
                }
                c => resolved.push(c),
            }
            match self.root.symlink_metadata(&resolved) {
                Ok(metadata) if metadata.is_symlink() => {
                    return invalid("must not pass through symlinks");
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(resolved)
    }
}

fn walk(dir: &Dir, prefix: &Path, files: &mut Vec<String>) -> io::Result<()> {
    for entry in dir.entries()? {
        let entry = entry?;
        let name = entry.file_name();
        let path = prefix.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() && !name.to_string_lossy().starts_with('.') {
            walk(&entry.open_dir()?, &path, files)?;
        } else if file_type.is_file() {
            files.push(path.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

// Case-insensitive, as vaults synced from macOS or Windows may differ in case.
fn is_config_dir(name: &str) -> bool {
    name.eq_ignore_ascii_case(CONFIG_DIR)
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::symlink};

    use tempfile::TempDir;

    use super::*;

    fn vault() -> (TempDir, Vault) {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("notes")).unwrap();
        fs::create_dir_all(dir.path().join(".obsidian")).unwrap();
        fs::write(dir.path().join("notes/a.md"), "a").unwrap();
        fs::write(dir.path().join(".obsidian/app.json"), "{}").unwrap();
        let vault = Vault::open(dir.path(), "AI").unwrap();
        (dir, vault)
    }

    fn assert_invalid(result: Result<impl fmt::Debug>) {
        assert!(
            matches!(result, Err(VaultError::InvalidPath(_))),
            "{result:?}"
        );
    }

    #[test]
    fn rejects_paths_leaving_vault() {
        let (_dir, vault) = vault();
        for path in [
            "../etc/passwd",
            "notes/../../x",
            "/etc/passwd",
            "notes//a.md",
            "",
        ] {
            assert_invalid(vault.read(path));
        }
    }

    #[test]
    fn rejects_config_directory() {
        let (dir, vault) = vault();
        assert_invalid(vault.read(".obsidian/app.json"));
        assert_invalid(vault.create(".Obsidian/new.json", "{}"));
        assert_invalid(Vault::open(dir.path(), ".obsidian").map(|_| ()));
        assert_invalid(vault.files(".obsidian"));
    }

    #[test]
    fn rejects_symlinks() {
        let (dir, vault) = vault();
        let outside = TempDir::new().unwrap();
        fs::write(outside.path().join("secret"), "s").unwrap();
        symlink(outside.path(), dir.path().join("escape")).unwrap();
        symlink(
            dir.path().join(".obsidian"),
            dir.path().join("notes/config"),
        )
        .unwrap();

        assert_invalid(vault.read("escape/secret"));
        assert_invalid(vault.read("notes/config/app.json"));
        symlink(dir.path().join("notes"), dir.path().join("AI")).unwrap();
        assert_invalid(vault.create("AI/b.md", "x"));
    }

    #[test]
    fn lists_files_without_dot_folders_or_symlinks() {
        let (dir, vault) = vault();
        symlink(dir.path().join("notes/a.md"), dir.path().join("link.md")).unwrap();
        fs::create_dir_all(dir.path().join("notes/sub")).unwrap();
        fs::write(dir.path().join("notes/sub/b.md"), "b").unwrap();
        fs::create_dir_all(dir.path().join(".trash")).unwrap();
        fs::write(dir.path().join(".trash/old.md"), "old").unwrap();
        fs::create_dir_all(dir.path().join("notes/.git")).unwrap();
        fs::write(dir.path().join("notes/.git/HEAD"), "ref").unwrap();

        assert_eq!(vault.files("").unwrap(), ["notes/a.md", "notes/sub/b.md"]);
        assert_eq!(vault.files("notes/sub").unwrap(), ["notes/sub/b.md"]);
    }

    #[test]
    fn writes_files() {
        let (_dir, vault) = vault();
        vault.create("AI/a.md", "a").unwrap();
        assert!(vault.create("AI/a.md", "x").is_err());
        vault.append("AI/a.md", "b").unwrap();
        vault.replace("AI/c.md", "c1").unwrap();
        vault.replace("AI/c.md", "c2").unwrap();

        assert_eq!(vault.read("AI/a.md").unwrap(), "ab");
        assert_eq!(vault.read("AI/c.md").unwrap(), "c2");
        assert_eq!(vault.files("AI").unwrap(), ["AI/a.md", "AI/c.md"]);
    }

    #[test]
    fn writes_only_inside_write_folder() {
        let (dir, vault) = vault();
        assert_invalid(vault.create("notes/b.md", "b"));
        assert_invalid(vault.append("notes/a.md", "b"));
        assert_invalid(vault.replace("notes/a.md", "b"));
        assert_invalid(vault.replace("AI", "b"));
        assert_invalid(vault.create("AIx/b.md", "b"));
        assert_eq!(
            fs::read_to_string(dir.path().join("notes/a.md")).unwrap(),
            "a"
        );
    }
}
