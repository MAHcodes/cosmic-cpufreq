use cosmic::cosmic_config::{
    self, Config, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry,
};

const CONFIG_VERSION: u64 = 1;

pub(crate) const APP_ID: &str = "dev.skylar.cosmic-ext-applet-cpufreq";

/// Which frequency value to display on the panel button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum FreqDisplayMode {
    Average,
    Minimum,
    Maximum,
}

impl Default for FreqDisplayMode {
    fn default() -> Self {
        Self::Average
    }
}

impl std::fmt::Display for FreqDisplayMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Average => write!(f, "Average"),
            Self::Minimum => write!(f, "Minimum"),
            Self::Maximum => write!(f, "Maximum"),
        }
    }
}

/// How to format frequency values shown in the UI and panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum FreqUnit {
    /// Show GHz above 1000 MHz, otherwise MHz.
    Auto,
    /// Always show gigahertz.
    Gigahertz,
    /// Always show megahertz.
    Megahertz,
}

impl Default for FreqUnit {
    fn default() -> Self {
        Self::Auto
    }
}

/// How the popup organizes the main content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum PopupLayout {
    /// Controls and Details as tabs.
    Tabbed,
    /// Single page with info and controls side by side.
    Horizontal,
    /// Single page with info above controls.
    Vertical,
}

impl Default for PopupLayout {
    fn default() -> Self {
        Self::Tabbed
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, CosmicConfigEntry)]
#[version = 1]
pub(crate) struct CpuFreqConfig {
    /// Whether turbo boost should be enabled
    pub(crate) turbo_enabled: bool,
    /// Update interval in milliseconds for the frequency display
    pub(crate) update_interval_ms: u64,
    /// Which frequency to show on the panel button
    pub(crate) freq_display_mode: FreqDisplayMode,
    /// How frequency values are formatted
    pub(crate) freq_unit: FreqUnit,
    /// Number of decimal places when displaying gigahertz
    pub(crate) freq_precision: u32,
    /// How the popup organizes the main content
    pub(crate) popup_layout: PopupLayout,
    /// Whether to show a CPU icon on the panel button
    pub(crate) show_panel_icon: bool,
    /// Whether to show the frequency text on the panel button
    pub(crate) show_panel_text: bool,
    /// Whether to show the per-core frequency grid
    pub(crate) show_per_core_freqs: bool,
    /// Whether to show per-core CPU usage % in the core grid
    pub(crate) show_per_core_usage: bool,
    /// Whether to show the aggregate CPU usage bar
    pub(crate) show_cpu_usage: bool,
    /// Whether to show the memory usage bar
    pub(crate) show_memory_usage: bool,
    /// Number of columns in the per-core frequency grid
    pub(crate) per_core_columns: u32,
    /// Whether to show the minimum frequency slider
    pub(crate) show_min_frequency_slider: bool,
    /// Whether to show the maximum frequency slider
    pub(crate) show_max_frequency_slider: bool,
    /// Whether to restore saved settings (turbo, governor) on launch
    pub(crate) restore_settings_on_start: bool,
    /// Last governor applied, restored on launch when `restore_settings_on_start`
    pub(crate) saved_governor: String,
}

impl Default for CpuFreqConfig {
    fn default() -> Self {
        Self {
            turbo_enabled: false,
            update_interval_ms: 2000,
            freq_display_mode: FreqDisplayMode::default(),
            freq_unit: FreqUnit::default(),
            freq_precision: 2,
            popup_layout: PopupLayout::default(),
            show_panel_icon: false,
            show_panel_text: true,
            show_per_core_freqs: true,
            show_per_core_usage: true,
            show_cpu_usage: true,
            show_memory_usage: true,
            per_core_columns: 4,
            show_min_frequency_slider: true,
            show_max_frequency_slider: true,
            restore_settings_on_start: true,
            saved_governor: String::new(),
        }
    }
}

impl CpuFreqConfig {
    fn config_handler() -> Option<Config> {
        Config::new(APP_ID, CONFIG_VERSION).ok()
    }

    fn config() -> CpuFreqConfig {
        match Self::config_handler() {
            Some(config_handler) => match CpuFreqConfig::get_entry(&config_handler) {
                Ok(config) => config,
                Err((errors, partial)) => {
                    for error in errors {
                        tracing::info!("Config field missing or invalid: {error}");
                    }
                    partial
                }
            },
            None => CpuFreqConfig::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Flags {
    pub(crate) config: CpuFreqConfig,
    pub(crate) config_handler: Option<cosmic_config::Config>,
}

impl Flags {
    pub(crate) fn new() -> Self {
        Self {
            config: CpuFreqConfig::config(),
            config_handler: CpuFreqConfig::config_handler(),
        }
    }
}
