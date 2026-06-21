/// Server configuration loaded from environment variables.
#[derive(Debug)]
pub struct Config {
    pub hsl_subscription_key: String,
    pub listen_addr: String,
    pub digitransit_endpoint: String,
    pub request_timeout_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let hsl_subscription_key = std::env::var("HSL_SUBSCRIPTION_KEY")
            .map_err(|_| "HSL_SUBSCRIPTION_KEY must be set".to_string())?;

        Ok(Config {
            hsl_subscription_key,
            listen_addr: std::env::var("LISTEN_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:3001".into()),
            digitransit_endpoint: std::env::var("DIGITRANSIT_ENDPOINT").unwrap_or_else(|_| {
                "https://api.digitransit.fi/routing/v2/hsl/gtfs/v1".into()
            }),
            request_timeout_secs: std::env::var("REQUEST_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// Single-threaded because it mutates process-global environment variables.
    #[test]
    #[serial]
    fn config_behaviour() {
        // Missing key => error
        unsafe { std::env::remove_var("HSL_SUBSCRIPTION_KEY") };
        let result = Config::from_env();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("HSL_SUBSCRIPTION_KEY"));

        // Defaults with only key set
        unsafe {
            std::env::set_var("HSL_SUBSCRIPTION_KEY", "k");
            std::env::remove_var("LISTEN_ADDR");
            std::env::remove_var("DIGITRANSIT_ENDPOINT");
            std::env::remove_var("REQUEST_TIMEOUT_SECS");
        }
        let config = Config::from_env().unwrap();
        assert_eq!(config.listen_addr, "0.0.0.0:3001");
        assert!(config.digitransit_endpoint.contains("api.digitransit.fi"));
        assert_eq!(config.request_timeout_secs, 30);

        // Custom listen addr
        unsafe { std::env::set_var("LISTEN_ADDR", "127.0.0.1:9999") };
        let config = Config::from_env().unwrap();
        assert_eq!(config.listen_addr, "127.0.0.1:9999");
    }
}
