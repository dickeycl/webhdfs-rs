//! File-based configuration.
//!
//! 1. Confiuration is read from one of the configuration files when `HdfsClientBuilder::from_config` or
//!    `HdfsClientBuilder::from_config` is called. The configuration files are never read or used unless
//!    explicitly requested.
//! 2. There are 3 locations where the library looks for the configuration information, in the order as
//!    listed below. The search is stopped on first file found, and this solely file is used. No configuration
//!    merging is currently supported.
//!    - If 'WEBHDFS_CONFIG' environment variable is set, then the location specfied by it is opened
//!      (must be a full file path). The library panics if there is no file at that path.
//!    - A file named 'webhdfs.toml' in the CWD.
//!    - A file named '.webhdfs.toml' in the user's home directory.
//! 3. If a file is found but is either unreadable or unparseable, then the library panics.
//! 4. All the configuration fields are optional, except for the entrypoint.
//! 5. Use `write_sample_config` to get config sample
//!
use crate::error::*;
use http::Uri;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::fs::read;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::time::Duration;

#[derive(Debug)]
pub struct UriW {
    uri: Uri,
}

impl UriW {
    pub fn new(uri: Uri) -> Self {
        Self { uri }
    }
    pub fn into_uri(self) -> Uri {
        self.uri
    }
}

impl<'de> Deserialize<'de> for UriW {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        let uri: Uri = s.parse().map_err(serde::de::Error::custom)?;
        Ok(UriW { uri })
    }
}

impl Serialize for UriW {
    fn serialize<S>(&self, serializer: S) -> StdResult<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.uri.to_string())
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HttpsConfig {
    pub danger_accept_invalid_certs: Option<bool>,
    pub danger_accept_invalid_hostnames: Option<bool>,
    pub use_sni: Option<bool>,
    pub identity_file: Option<String>,
    pub identity_password: Option<String>,
    pub min_protocol_version: Option<String>,
    pub max_protocol_version: Option<String>,
    pub root_certificates: Option<Vec<String>>,
}

impl Default for HttpsConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpsConfig {
    pub fn new() -> Self {
        Self {
            danger_accept_invalid_certs: None,
            danger_accept_invalid_hostnames: None,
            use_sni: None,
            identity_file: None,
            identity_password: None,
            min_protocol_version: None,
            max_protocol_version: None,
            root_certificates: None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub entrypoint: UriW,
    pub alt_entrypoint: Option<UriW>,
    pub default_timeout: Option<Duration>,
    pub user_name: Option<String>,
    pub doas: Option<String>,
    pub dt: Option<String>,
    pub natmap: Option<HashMap<String, String>>,
    pub https_config: Option<HttpsConfig>,
}

impl Config {
    pub fn new(uri: Uri) -> Self {
        Self {
            entrypoint: UriW::new(uri),
            alt_entrypoint: None,
            default_timeout: None,
            user_name: None,
            doas: None,
            dt: None,
            natmap: None,
            https_config: None,
        }
    }
}

#[test]
fn test_config() {
    let cfg_str = br#"
entrypoint="http://localhost:7000"
duration="10s"
user_name="us"
"#;
    let c: Config = toml::from_slice(cfg_str).unwrap();
    assert_eq!(c.entrypoint.uri, "http://localhost:7000")
}

#[cfg(windows)]
#[inline]
fn get_home_dir() -> Option<String> {
    let d: Option<String> = std::env::var("HOMEDRIVE").ok();
    let p: Option<String> = std::env::var("HOMEPATH").ok();
    d.and_then(|d| p.map(|p| d + &p))
}

#[cfg(not(windows))]
#[inline]
fn get_home_dir() -> Option<String> {
    std::env::var("HOME").ok()
}

fn read_local_config() -> Result<Option<Config>> {
    let p = Path::new("webhdfs.toml");
    if p.is_file() {
        Ok(Some(toml::from_slice(&read(p)?)?))
    } else {
        Ok(None)
    }
}

fn read_user_config() -> Result<Option<Config>> {
    match get_home_dir() {
        None => Ok(None),
        Some(f) => {
            let p = Path::new(&f);
            let p = p.join(Path::new(".webhdfs.toml"));
            if p.is_file() {
                Ok(Some(toml::from_slice(&read(p)?)?))
            } else {
                Ok(None)
            }
        }
    }
}

fn read_env_config() -> Result<Option<Config>> {
    match std::env::var("WEBHDFS_CONFIG").ok() {
        None => Ok(None),
        Some(f) => {
            let p = Path::new(&f);
            Ok(Some(toml::from_slice(&read(p)?)?))
        }
    }
}

pub fn read_config() -> Config {
    read_config_opt().expect("No valid webhdfs configuration file has been found")
}

pub fn read_config_opt() -> Option<Config> {
    read_env_config()
        .expect("Configuration error (file specified by WEBHDFS_CONFIG environment var)")
        .or_else(|| read_local_config().expect("Configuration error (webhdfs.toml in CWD)"))
        .or_else(|| read_user_config().expect("Configuration error (.webhdfs.toml in homedir)"))
}

pub fn write_config(path: &Path, c: &Config, new_file: bool) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create_new(new_file)
        .open(path)
        .unwrap();
    f.write_all(&toml::to_vec(c).unwrap()).unwrap();
}
/*
pub fn write_sample_config() {
    let c = Config {
        entrypoint: UriW::new("http://namenode.hdfs.intra:50070".parse().unwrap()),
        default_timeout: Some(Duration::from_secs(30)),
        user_name: Some("dr.who".to_owned()),
        doas: Some("doas.user".to_owned()),
        dt: Some("---encoded-delegation-token---".to_owned())
    };
    write_config(&Path::new("template.webhdfs.toml"), &c, false)
}
*/

/// Splits a "key=value" string in two parts
/// ```
/// use webhdfs::config::split_kv;
/// assert_eq!(split_kv("key=Value".to_owned()).unwrap(), ("key".to_owned(), "Value".to_owned()))
/// ```
#[inline]
pub fn split_kv(l: String) -> Result<(String, String)> {
    let mut fs = l.splitn(2, '=');
    let a = fs
        .next()
        .ok_or_else(|| app_error!(generic "cannot read entry key: {}", l))?
        .to_owned();
    let b = fs
        .next()
        .ok_or_else(|| app_error!(generic "cannot read entry value: {}", l))?
        .to_owned();
    Ok((a, b))
}

/// Reads an object consisting of "key=value" pairs
#[inline]
pub fn read_kv_lines<R: Read>(r: R) -> Result<HashMap<String, String>> {
    //let f = File::open(path).aerr("cannot open natmap")?;
    let f = BufReader::new(r);
    let r = f.lines().map(|l| {
        let ln = l.aerr("cannot read natmap line")?;
        split_kv(ln)
    });
    r.collect()
}

#[test]
fn test_read_kv_lines() {
    let input = b"\
bigtop1.vagrant:50070=localhost:51070
bigtop1.vagrant:50075=localhost:51075
";
    let r = read_kv_lines(&input[..]).unwrap();
    assert_eq!(
        r.get("bigtop1.vagrant:50070").map(|r| r.as_ref()),
        Some("localhost:51070")
    );
    assert_eq!(
        r.get("bigtop1.vagrant:50075").map(|r| r.as_ref()),
        Some("localhost:51075")
    );
}

#[inline]
pub fn read_kv_file(path: &str) -> Result<HashMap<String, String>> {
    read_kv_lines(std::fs::File::open(path).aerr("cannot open natmap")?)
}
