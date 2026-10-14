use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaileysConfig {
    pub browser_name: String,
    pub platform: String,
    pub browser_version: String,
    pub wa_websocket_url: String,
    pub connect_timeout_ms: u64,
    pub keep_alive_interval_ms: u64,
    pub mark_online_on_connect: bool,
    pub phone_number_pairing: Option<String>,
    pub app_version: (u32, u32, u32),
    pub dynamic_version: bool,
    /// Anti-Revoke: Preserves original message summary and prevents deletion of messages when a revoke protocol message is received.
    pub anti_revoke: bool,
    /// Anti View-Once: Intercepts and automatically downloads view-once media attachments to local storage.
    pub auto_save_view_once: bool,
    /// Ghost Mode: Suppresses read receipts and status view receipts towards other contacts.
    pub ghost_mode: bool,
}

impl Default for BaileysConfig {
    fn default() -> Self {
        Self {
            browser_name: "Chrome".to_string(),
            platform: "Mac OS".to_string(),
            browser_version: "14.4.1".to_string(),
            wa_websocket_url: "wss://web.whatsapp.com/ws/chat".to_string(),
            connect_timeout_ms: 20_000,
            keep_alive_interval_ms: 20_000,
            mark_online_on_connect: true,
            phone_number_pairing: None,
            app_version: (2, 3000, 1043857760),
            dynamic_version: true,
            anti_revoke: true,
            auto_save_view_once: true,
            ghost_mode: false,
        }
    }
}

impl BaileysConfig {
    pub fn builder() -> BaileysConfigBuilder {
        BaileysConfigBuilder::new()
    }
}

pub struct BaileysConfigBuilder {
    config: BaileysConfig,
}

impl Default for BaileysConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl BaileysConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: BaileysConfig::default(),
        }
    }

    pub fn with_browser(
        mut self,
        name: impl Into<String>,
        platform: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        self.config.browser_name = name.into();
        self.config.platform = platform.into();
        self.config.browser_version = version.into();
        self
    }

    pub fn with_version(mut self, primary: u32, secondary: u32, tertiary: u32) -> Self {
        self.config.app_version = (primary, secondary, tertiary);
        self
    }

    pub fn with_dynamic_version(mut self, dynamic_version: bool) -> Self {
        self.config.dynamic_version = dynamic_version;
        self
    }

    pub fn with_anti_revoke(mut self, anti_revoke: bool) -> Self {
        self.config.anti_revoke = anti_revoke;
        self
    }

    pub fn with_auto_save_view_once(mut self, auto_save_view_once: bool) -> Self {
        self.config.auto_save_view_once = auto_save_view_once;
        self
    }

    pub fn with_ghost_mode(mut self, ghost_mode: bool) -> Self {
        self.config.ghost_mode = ghost_mode;
        self
    }

    pub fn with_websocket_url(mut self, url: impl Into<String>) -> Self {
        self.config.wa_websocket_url = url.into();
        self
    }

    pub fn with_pairing_phone_number(mut self, phone_number: impl Into<String>) -> Self {
        self.config.phone_number_pairing = Some(phone_number.into());
        self
    }

    pub fn with_phone_pairing(self, phone_number: impl Into<String>) -> Self {
        self.with_pairing_phone_number(phone_number)
    }

    /// Automatically sets the platform name according to the runtime host OS.
    pub fn with_current_platform(mut self) -> Self {
        #[cfg(target_os = "windows")]
        {
            self.config.platform = "Windows".to_string();
        }
        #[cfg(target_os = "macos")]
        {
            self.config.platform = "Mac OS".to_string();
        }
        #[cfg(target_os = "linux")]
        {
            self.config.platform = "Linux".to_string();
        }
        #[cfg(target_os = "android")]
        {
            self.config.platform = "Android".to_string();
        }
        #[cfg(target_os = "ios")]
        {
            self.config.platform = "iOS".to_string();
        }
        #[cfg(not(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux",
            target_os = "android",
            target_os = "ios"
        )))]
        {
            self.config.platform = std::env::consts::OS.to_string();
        }
        self
    }

    pub fn build(self) -> BaileysConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let config = BaileysConfig::builder().with_current_platform().build();
        #[cfg(target_os = "windows")]
        assert_eq!(config.platform, "Windows");
        #[cfg(target_os = "macos")]
        assert_eq!(config.platform, "Mac OS");
        #[cfg(target_os = "linux")]
        assert_eq!(config.platform, "Linux");
        #[cfg(target_os = "android")]
        assert_eq!(config.platform, "Android");
        #[cfg(target_os = "ios")]
        assert_eq!(config.platform, "iOS");
    }
}
