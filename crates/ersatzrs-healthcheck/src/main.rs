use std::env;
use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::process::ExitCode;
use std::time::Duration;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8409;
const DEFAULT_PATH: &str = "/healthz";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_USER_AGENT: &str = "ersatzrs-healthcheck/0.1";

#[derive(Debug, Clone, PartialEq, Eq)]
struct HealthcheckConfig {
    target: HttpTarget,
    timeout: Duration,
}

impl Default for HealthcheckConfig {
    fn default() -> Self {
        Self {
            target: HttpTarget {
                host: DEFAULT_HOST.to_owned(),
                port: DEFAULT_PORT,
                path: DEFAULT_PATH.to_owned(),
            },
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpTarget {
    host: String,
    port: u16,
    path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProbeOutcome {
    Healthy,
    Unhealthy(String),
    InvalidConfig(String),
}

impl ProbeOutcome {
    fn exit_code(&self) -> ExitCode {
        match self {
            Self::Healthy => ExitCode::SUCCESS,
            Self::Unhealthy(_) => ExitCode::FAILURE,
            Self::InvalidConfig(_) => ExitCode::from(2),
        }
    }

    fn message(&self) -> Option<&str> {
        match self {
            Self::Healthy => None,
            Self::Unhealthy(message) | Self::InvalidConfig(message) => Some(message),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConfigError {
    MissingValue(String),
    InvalidFlag(String),
    InvalidPort(String),
    InvalidTimeout(String),
    InvalidUrl(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingValue(flag) => write!(formatter, "missing value for {flag}"),
            Self::InvalidFlag(flag) => write!(formatter, "unknown argument {flag}"),
            Self::InvalidPort(value) => write!(formatter, "invalid port {value:?}"),
            Self::InvalidTimeout(value) => write!(formatter, "invalid timeout {value:?}"),
            Self::InvalidUrl(value) => write!(formatter, "invalid URL {value:?}"),
        }
    }
}

fn main() -> ExitCode {
    let outcome = match HealthcheckConfig::from_env_and_args(env::args().skip(1)) {
        Ok(config) => probe(&config),
        Err(error) => ProbeOutcome::InvalidConfig(error.to_string()),
    };

    if let Some(message) = outcome.message() {
        eprintln!("{message}");
    }

    outcome.exit_code()
}

impl HealthcheckConfig {
    #[allow(clippy::disallowed_methods)]
    fn from_env_and_args(args: impl IntoIterator<Item = String>) -> Result<Self, ConfigError> {
        let mut config = Self::default();

        if let Ok(url) = env::var("ERSATZRS_HEALTHCHECK_URL") {
            config.target = parse_http_url(&url)?;
        }

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--url" => {
                    let url = args
                        .next()
                        .ok_or_else(|| ConfigError::MissingValue(arg.clone()))?;
                    config.target = parse_http_url(&url)?;
                }
                "--host" => {
                    config.target.host = args
                        .next()
                        .ok_or_else(|| ConfigError::MissingValue(arg.clone()))?;
                }
                "--port" => {
                    let value = args
                        .next()
                        .ok_or_else(|| ConfigError::MissingValue(arg.clone()))?;
                    config.target.port = parse_port(&value)?;
                }
                "--path" => {
                    let value = args
                        .next()
                        .ok_or_else(|| ConfigError::MissingValue(arg.clone()))?;
                    config.target.path = normalize_path(&value);
                }
                "--timeout-ms" => {
                    let value = args
                        .next()
                        .ok_or_else(|| ConfigError::MissingValue(arg.clone()))?;
                    config.timeout = parse_timeout_ms(&value)?;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => return Err(ConfigError::InvalidFlag(arg)),
            }
        }

        Ok(config)
    }
}

fn probe(config: &HealthcheckConfig) -> ProbeOutcome {
    match read_http_status(&config.target, config.timeout) {
        Ok(status) => outcome_for_status(&config.target, status),
        Err(error) => ProbeOutcome::Unhealthy(format!(
            "health check failed for {}: {error}",
            config.target
        )),
    }
}

fn outcome_for_status(target: &HttpTarget, status: u16) -> ProbeOutcome {
    if status == 200 {
        return ProbeOutcome::Healthy;
    }

    ProbeOutcome::Unhealthy(format!("health check returned HTTP {status} for {target}"))
}

fn read_http_status(target: &HttpTarget, timeout: Duration) -> std::io::Result<u16> {
    let address = (target.host.as_str(), target.port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::other("host did not resolve"))?;
    let mut stream = TcpStream::connect_timeout(&address, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}:{}\r\nUser-Agent: {DEFAULT_USER_AGENT}\r\nConnection: close\r\n\r\n",
        target.path, target.host, target.port
    );
    stream.write_all(request.as_bytes())?;

    let mut response = String::new();
    stream.take(8192).read_to_string(&mut response)?;
    parse_status_line(&response).ok_or_else(|| std::io::Error::other("missing HTTP status line"))
}

fn parse_status_line(response: &str) -> Option<u16> {
    let status = response.lines().next()?.split_whitespace().nth(1)?;
    status.parse().ok()
}

fn parse_http_url(value: &str) -> Result<HttpTarget, ConfigError> {
    let Some(rest) = value.strip_prefix("http://") else {
        return Err(ConfigError::InvalidUrl(value.to_owned()));
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, DEFAULT_PATH));
    if authority.is_empty() {
        return Err(ConfigError::InvalidUrl(value.to_owned()));
    }

    let (host, port) = if let Some((host, port)) = authority.rsplit_once(':') {
        if host.is_empty() {
            return Err(ConfigError::InvalidUrl(value.to_owned()));
        }
        (host.to_owned(), parse_port(port)?)
    } else {
        (authority.to_owned(), 80)
    };

    Ok(HttpTarget {
        host,
        port,
        path: normalize_path(path),
    })
}

fn normalize_path(value: &str) -> String {
    if value.starts_with('/') {
        value.to_owned()
    } else {
        format!("/{value}")
    }
}

fn parse_port(value: &str) -> Result<u16, ConfigError> {
    let port = value
        .parse::<u16>()
        .map_err(|_| ConfigError::InvalidPort(value.to_owned()))?;
    if port == 0 {
        return Err(ConfigError::InvalidPort(value.to_owned()));
    }
    Ok(port)
}

fn parse_timeout_ms(value: &str) -> Result<Duration, ConfigError> {
    let millis = value
        .parse::<u64>()
        .map_err(|_| ConfigError::InvalidTimeout(value.to_owned()))?;
    if millis == 0 {
        return Err(ConfigError::InvalidTimeout(value.to_owned()));
    }
    Ok(Duration::from_millis(millis))
}

fn print_help() {
    println!(
        "\
ersatzrs-healthcheck

Checks ErsatzRS readiness with an HTTP GET.

Options:
  --url <URL>          Full http:// URL to probe.
  --host <HOST>        Host to probe when --url is not used. Default: {DEFAULT_HOST}
  --port <PORT>        Port to probe when --url is not used. Default: {DEFAULT_PORT}
  --path <PATH>        Path to probe when --url is not used. Default: {DEFAULT_PATH}
  --timeout-ms <MS>    TCP/read/write timeout. Default: 5000

Environment:
  ERSATZRS_HEALTHCHECK_URL overrides the default URL.
"
    );
}

impl fmt::Display for HttpTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "http://{}:{}{}", self.host, self.port, self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_targets_docker_healthz() {
        let config = HealthcheckConfig::from_env_and_args(Vec::new()).expect("config");

        assert_eq!(config.target.host, "127.0.0.1");
        assert_eq!(config.target.port, 8409);
        assert_eq!(config.target.path, "/healthz");
        assert_eq!(config.timeout, Duration::from_secs(5));
    }

    #[test]
    fn url_overrides_default_target() {
        let config = HealthcheckConfig::from_env_and_args([
            "--url".to_owned(),
            "http://localhost:18409/system/health".to_owned(),
            "--timeout-ms".to_owned(),
            "250".to_owned(),
        ])
        .expect("config");

        assert_eq!(
            config.target,
            HttpTarget {
                host: "localhost".to_owned(),
                port: 18409,
                path: "/system/health".to_owned(),
            }
        );
        assert_eq!(config.timeout, Duration::from_millis(250));
    }

    #[test]
    fn host_port_path_build_a_target_without_url() {
        let config = HealthcheckConfig::from_env_and_args([
            "--host".to_owned(),
            "localhost".to_owned(),
            "--port".to_owned(),
            "8410".to_owned(),
            "--path".to_owned(),
            "healthz".to_owned(),
        ])
        .expect("config");

        assert_eq!(
            config.target,
            HttpTarget {
                host: "localhost".to_owned(),
                port: 8410,
                path: "/healthz".to_owned(),
            }
        );
    }

    #[test]
    fn invalid_port_is_configuration_failure() {
        let error = HealthcheckConfig::from_env_and_args(["--port".to_owned(), "0".to_owned()])
            .expect_err("invalid port");

        assert_eq!(error, ConfigError::InvalidPort("0".to_owned()));
    }

    #[test]
    fn exit_codes_distinguish_health_and_configuration() {
        assert_eq!(ProbeOutcome::Healthy.exit_code(), ExitCode::SUCCESS);
        assert_eq!(
            ProbeOutcome::Unhealthy("down".to_owned()).exit_code(),
            ExitCode::FAILURE
        );
        assert_eq!(
            ProbeOutcome::InvalidConfig("bad".to_owned()).exit_code(),
            ExitCode::from(2)
        );
    }

    #[test]
    fn parses_http_status_line() {
        assert_eq!(
            parse_status_line("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n"),
            Some(200)
        );
        assert_eq!(parse_status_line(""), None);
    }

    #[test]
    fn status_200_maps_to_healthy() {
        let outcome = outcome_for_status(&default_target(), 200);

        assert_eq!(outcome, ProbeOutcome::Healthy);
    }

    #[test]
    fn status_503_maps_to_unhealthy() {
        let outcome = outcome_for_status(&default_target(), 503);

        assert!(matches!(outcome, ProbeOutcome::Unhealthy(message) if message.contains("503")));
    }

    fn default_target() -> HttpTarget {
        HttpTarget {
            host: "127.0.0.1".to_owned(),
            port: 8409,
            path: "/healthz".to_owned(),
        }
    }
}
