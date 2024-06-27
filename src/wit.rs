use super::common::*;
use super::config::*;
use super::model::*;
use super::progress::*;
use super::util::*;
use super::SimpleResult;
use indicatif::ProgressBar;
use sqlx::SqliteConnection;
use lazy_static::lazy_static;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

const WIT: &str = "wit";

lazy_static! {
    static ref VERSION_REGEX: Regex = Regex::new(r"\d+\.[\d\w]+").unwrap();
}

pub struct WbfsRomfile {
    pub path: PathBuf,
}

impl AsCommon for WbfsRomfile {
    fn as_common(&self) -> SimpleResult<CommonRomfile> {
        CommonRomfile::from_path(&self.path)
    }
}

pub trait ToWbfs {
    async fn to_wbfs<P: AsRef<Path>>(
        &self,
        progress_bar: &ProgressBar,
        destination_directory: &P,
    ) -> SimpleResult<WbfsRomfile>;
}

impl Check for WbfsRomfile {
    async fn check(
        &self,
        connection: &mut SqliteConnection,
        progress_bar: &ProgressBar,
        header: &Option<Header>,
        roms: &[&Rom],
        hash_algorithm: &HashAlgorithm,
    ) -> SimpleResult<()> {
        progress_bar.println(format!("Checking \"{}\"", self.as_common()?.to_string()));
        let tmp_directory = create_tmp_directory(connection).await?;
        let iso_romfile = self.to_iso(progress_bar, &tmp_directory.path()).await?;
        iso_romfile
            .as_common()?
            .check(connection, progress_bar, header, roms, hash_algorithm)
            .await?;
        Ok(())
    }
}

impl ToIso for WbfsRomfile {
    async fn to_iso<P: AsRef<Path>>(
        &self,
        progress_bar: &ProgressBar,
        destination_directory: &P,
    ) -> SimpleResult<IsoRomfile> {
        progress_bar.set_message("Extracting wbfs");
        progress_bar.set_style(get_none_progress_style());
        progress_bar.enable_steady_tick(Duration::from_millis(100));

        progress_bar.println(format!(
            "Extracting \"{}\"",
            self.path.file_name().unwrap().to_str().unwrap()
        ));

        let path = destination_directory
            .as_ref()
            .join(self.path.file_name().unwrap())
            .with_extension(ISO_EXTENSION);

        let output = Command::new(WIT)
            .arg("COPY")
            .arg("--iso")
            .arg("--source")
            .arg(&self.path)
            .arg("--dest")
            .arg(&path)
            .output()
            .await
            .expect("Failed to extract wbfs");

        if !output.status.success() {
            bail!(String::from_utf8(output.stderr).unwrap().as_str())
        }

        progress_bar.set_message("");
        progress_bar.disable_steady_tick();

        Ok(IsoRomfile { path })
    }
}

impl ToWbfs for IsoRomfile {
    async fn to_wbfs<P: AsRef<Path>>(
        &self,
        progress_bar: &ProgressBar,
        destination_directory: &P,
    ) -> simple_error::SimpleResult<WbfsRomfile> {
        progress_bar.set_message("Creating wbfs");
        progress_bar.set_style(get_none_progress_style());
        progress_bar.enable_steady_tick(Duration::from_millis(100));

        let path = destination_directory
            .as_ref()
            .join(self.path.file_name().unwrap())
            .with_extension(WBFS_EXTENSION);

        let output = Command::new(WIT)
            .arg("COPY")
            .arg("--wbfs")
            .arg("--source")
            .arg(&self.path)
            .arg("--dest")
            .arg(&path)
            .output()
            .await
            .expect("Failed to create wbfs");

        if !output.status.success() {
            bail!(String::from_utf8(output.stderr).unwrap().as_str())
        }

        progress_bar.set_message("");
        progress_bar.disable_steady_tick();

        Ok(WbfsRomfile { path })
    }
}

pub async fn get_version() -> SimpleResult<String> {
    let output = try_with!(
        Command::new(WIT).arg("--version").output().await,
        "Failed to spawn wit"
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    let version = stdout
        .lines()
        .next()
        .and_then(|line| VERSION_REGEX.find(line))
        .map(|version| version.as_str().to_string())
        .unwrap_or(String::from("unknown"));

    Ok(version)
}

impl FromPath<WbfsRomfile> for WbfsRomfile {
    fn from_path<P: AsRef<Path>>(path: &P) -> SimpleResult<WbfsRomfile> {
        let path = path.as_ref().to_path_buf();
        let extension = path.extension().unwrap().to_str().unwrap().to_lowercase();
        if extension != WBFS_EXTENSION {
            bail!("Not a valid wbfs");
        }
        Ok(WbfsRomfile { path })
    }
}

pub trait AsWbfs {
    fn as_wbfs(&self) -> SimpleResult<WbfsRomfile>;
}

impl AsWbfs for Romfile {
    fn as_wbfs(&self) -> SimpleResult<WbfsRomfile> {
        WbfsRomfile::from_path(&self.path)
    }
}