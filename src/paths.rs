use directories::BaseDirs;
use std::fs;
use std::io;
use std::path::PathBuf;

/// Files shared with the existing Python implementation.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub app_dir: PathBuf,
    pub config_file: PathBuf,
    pub token_cache_file: PathBuf,
    pub db_file: PathBuf,
}

impl AppPaths {
    pub fn for_home(home: PathBuf) -> Self {
        let app_dir = home.join(".psa-tool");
        Self {
            config_file: app_dir.join("config.json"),
            token_cache_file: app_dir.join("token_cache.json"),
            db_file: app_dir.join("psa.sqlite3"),
            app_dir,
        }
    }

    pub fn discover() -> io::Result<Self> {
        let base_dirs = BaseDirs::new().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "home directory could not be determined",
            )
        })?;
        Ok(Self::for_home(base_dirs.home_dir().to_path_buf()))
    }

    /// Creates the private application directory used by both implementations.
    #[cfg(unix)]
    pub fn ensure_app_dir(&self) -> io::Result<()> {
        use std::os::unix::fs::DirBuilderExt;

        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.app_dir)
    }

    #[cfg(not(unix))]
    pub fn ensure_app_dir(&self) -> io::Result<()> {
        fs::create_dir_all(&self.app_dir)
    }
}
