use std::time::Duration;

use tracing::{debug, trace};

use crate::{
    config::{APP_ID, CpuFreqConfig, Flags, FreqDisplayMode, FreqUnit, PopupLayout},
    cpu, fl,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MainTab {
    Controls,
    Details,
}

pub(crate) fn run() -> cosmic::iced::Result {
    cosmic::applet::run::<CpuFreqApplet>(Flags::new())
}

const DEFAULT_POPUP_WIDTH: f32 = 360.0;
const HORIZONTAL_POPUP_WIDTH: f32 = 600.0;
/// Fixed width reserved for the panel frequency text so it never changes width.
const PANEL_TEXT_WIDTH: f32 = 64.0;

struct CpuFreqApplet {
    core: cosmic::app::Core,
    popup: Option<cosmic::iced::window::Id>,
    config: CpuFreqConfig,
    config_handler: Option<cosmic::cosmic_config::Config>,
    // Live state
    current_freq_mhz: Option<f64>,
    turbo_enabled: Option<bool>,
    current_governor: Option<String>,
    available_governors: Vec<String>,
    current_epp: Option<String>,
    available_epp: Vec<String>,
    /// (hw_min_mhz, turbo_max_mhz) - the full range including turbo
    freq_bounds: Option<(f64, f64)>,
    scaling_min_mhz: Option<f64>,
    scaling_max_mhz: Option<f64>,
    /// Preview values while dragging sliders (not yet committed to sysfs)
    preview_min_mhz: Option<f64>,
    preview_max_mhz: Option<f64>,
    update_interval_input: String,
    show_settings: bool,
    // Tab model for the main popup (Controls / Details)
    main_tab_model: cosmic::widget::segmented_button::SingleSelectModel,
    // System info (static, read once)
    cpu_model: Option<String>,
    machine_vendor: Option<String>,
    machine_model: Option<String>,
    os_name: Option<String>,
    kernel_version: Option<String>,
    // Per-core and usage (updated only when popup is open)
    per_core_freqs: Vec<(u32, f64)>,
    cpu_usage_prev: Option<cpu::CpuUsageSnapshot>,
    cpu_usage_percent: Option<f64>,
    per_core_usage: Vec<f64>,
    memory_used_kb: Option<u64>,
    memory_total_kb: Option<u64>,
    // Dropdown options stored to satisfy borrow lifetime
    display_mode_options: Vec<String>,
    freq_unit_options: Vec<String>,
    precision_options: Vec<String>,
    columns_options: Vec<String>,
    layout_options: Vec<String>,
    governor_labels: Vec<String>,
    epp_labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) enum Message {
    Tick,
    ToggleWindow,
    PopupClosed(cosmic::iced::window::Id),
    Surface(cosmic::surface::Action),
    SetTurbo(bool),
    SetGovernor(usize),
    SetEpp(usize),
    /// Preview while dragging (UI only, no sysfs write)
    PreviewScalingMin(f64),
    PreviewScalingMax(f64),
    /// Commit on slider release
    CommitScalingMin,
    CommitScalingMax,
    UpdateIntervalChanged(String),
    ToggleSettings,
    MainTabActivated(cosmic::widget::segmented_button::Entity),
    SetFreqDisplayMode(usize),
    SetFreqUnit(usize),
    SetFreqPrecision(usize),
    SetPerCoreColumns(usize),
    SetLayout(usize),
    SetShowPanelIcon(bool),
    SetShowPanelText(bool),
    SetShowPerCoreFreqs(bool),
    SetShowPerCoreUsage(bool),
    SetShowCpuUsage(bool),
    SetShowMemoryUsage(bool),
    SetShowMinFrequencySlider(bool),
    SetShowMaxFrequencySlider(bool),
    SetRestoreOnStart(bool),
}

impl cosmic::Application for CpuFreqApplet {
    type Flags = Flags;
    type Message = Message;
    type Executor = cosmic::SingleThreadExecutor;

    const APP_ID: &'static str = APP_ID;

    fn init(
        core: cosmic::app::Core,
        flags: Self::Flags,
    ) -> (Self, cosmic::app::Task<Self::Message>) {
        let turbo_enabled = cpu::read_turbo_enabled();
        let current_governor = cpu::read_current_governor();
        let available_governors = cpu::read_available_governors();
        let current_epp = cpu::read_current_epp();
        let available_epp = cpu::read_available_epp();
        let min_freq = cpu::read_min_frequency_mhz();
        let max_freq = cpu::read_effective_max_frequency_mhz();
        let freq_bounds = min_freq.zip(max_freq);
        let scaling_min_mhz = cpu::read_scaling_min_mhz();
        let scaling_max_mhz = cpu::read_scaling_max_mhz();
        let current_freq_mhz = cpu::read_current_frequency_mhz();

        let update_interval_input = flags.config.update_interval_ms.to_string();

        let cpu_model = cpu::read_cpu_model();
        let (machine_vendor, machine_model) = cpu::read_machine_info()
            .map(|(v, m)| (Some(v), Some(m)))
            .unwrap_or((None, None));
        let os_name = cpu::read_os_name();
        let kernel_version = cpu::read_kernel_version();

        let mut main_tab_model = cosmic::widget::segmented_button::Model::builder()
            .insert(|b| b.text(fl!("tab-controls")).data(MainTab::Controls))
            .insert(|b| b.text(fl!("tab-details")).data(MainTab::Details))
            .build();
        main_tab_model.activate_position(0);

        let mut applet = Self {
            core,
            popup: None,
            config: flags.config,
            config_handler: flags.config_handler,
            current_freq_mhz,
            turbo_enabled,
            current_governor,
            available_governors,
            current_epp,
            available_epp,
            freq_bounds,
            scaling_min_mhz,
            scaling_max_mhz,
            preview_min_mhz: None,
            preview_max_mhz: None,
            update_interval_input,
            show_settings: false,
            main_tab_model,
            cpu_model,
            machine_vendor,
            machine_model,
            os_name,
            kernel_version,
            per_core_freqs: Vec::new(),
            cpu_usage_prev: None,
            cpu_usage_percent: None,
            per_core_usage: Vec::new(),
            memory_used_kb: None,
            memory_total_kb: None,
            display_mode_options: vec![
                fl!("display-average"),
                fl!("display-minimum"),
                fl!("display-maximum"),
            ],
            freq_unit_options: vec![fl!("unit-auto"), fl!("unit-ghz"), fl!("unit-mhz")],
            precision_options: (0..=3).map(|p| p.to_string()).collect(),
            columns_options: ["2", "3", "4", "6", "8"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            layout_options: vec![
                fl!("layout-tabbed"),
                fl!("layout-horizontal"),
                fl!("layout-vertical"),
            ],
            governor_labels: Vec::new(),
            epp_labels: Vec::new(),
        };

        applet.rebuild_labels();

        // Restore saved settings if the system state differs
        if applet.config.restore_settings_on_start {
            applet.restore_turbo_setting();
            applet.restore_governor_setting();
        }

        (applet, cosmic::task::none())
    }

    fn core(&self) -> &cosmic::app::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::app::Core {
        &mut self.core
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Message> {
        cosmic::iced::time::every(Duration::from_millis(self.config.update_interval_ms))
            .map(|_| Message::Tick)
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: cosmic::iced::window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Message) -> cosmic::app::Task<Self::Message> {
        match message {
            Message::Tick => trace!(?message),
            _ => debug!(?message),
        }

        match message {
            Message::Tick => {
                // Always read per-core so we can derive avg/min/max for panel
                self.per_core_freqs = cpu::read_per_core_frequencies_mhz();
                self.current_freq_mhz = if self.per_core_freqs.is_empty() {
                    None
                } else {
                    let freqs = self.per_core_freqs.iter().map(|(_, mhz)| *mhz);
                    match self.config.freq_display_mode {
                        FreqDisplayMode::Average => {
                            let total: f64 = freqs.clone().sum();
                            Some(total / self.per_core_freqs.len() as f64)
                        }
                        FreqDisplayMode::Minimum => freqs.reduce(f64::min),
                        FreqDisplayMode::Maximum => freqs.reduce(f64::max),
                    }
                };
                let system_turbo = cpu::read_turbo_enabled();
                // If turbo was changed externally, restore our saved setting
                if self.config.restore_settings_on_start
                    && system_turbo != Some(self.config.turbo_enabled)
                    && self.turbo_enabled == Some(self.config.turbo_enabled)
                    && system_turbo != self.turbo_enabled
                {
                    tracing::info!(
                        "Turbo changed externally (now {:?}), restoring saved setting: {}",
                        system_turbo,
                        self.config.turbo_enabled
                    );
                    if cpu::write_turbo(self.config.turbo_enabled).is_ok() {
                        self.turbo_enabled = Some(self.config.turbo_enabled);
                    } else {
                        self.turbo_enabled = system_turbo;
                    }
                } else {
                    self.turbo_enabled = system_turbo;
                }
                self.current_governor = cpu::read_current_governor();
                self.current_epp = cpu::read_current_epp();
                // Only update from sysfs if not currently dragging a slider
                if self.preview_min_mhz.is_none() {
                    self.scaling_min_mhz = cpu::read_scaling_min_mhz();
                }
                if self.preview_max_mhz.is_none() {
                    self.scaling_max_mhz = cpu::read_scaling_max_mhz();
                }

                // Only poll usage data when popup is open
                if self.popup.is_some() {
                    if let Some(snapshot) = cpu::read_cpu_usage_snapshot() {
                        if let Some(prev) = &self.cpu_usage_prev {
                            self.cpu_usage_percent = Some(snapshot.usage_percent(prev));
                            self.per_core_usage = snapshot.per_core_usage_percent(prev);
                        }
                        self.cpu_usage_prev = Some(snapshot);
                    }

                    if let Some((used, total)) = cpu::read_memory_usage() {
                        self.memory_used_kb = Some(used);
                        self.memory_total_kb = Some(total);
                    }
                }
            }
            Message::ToggleWindow => {
                if let Some(id) = self.popup.take() {
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(
                        id,
                    );
                }

                // Refresh data when opening popup
                self.show_settings = false;
                self.available_governors = cpu::read_available_governors();
                self.available_epp = cpu::read_available_epp();
                self.rebuild_labels();
                let min_freq = cpu::read_min_frequency_mhz();
                let max_freq = cpu::read_effective_max_frequency_mhz();
                self.freq_bounds = min_freq.zip(max_freq);
                self.per_core_freqs = cpu::read_per_core_frequencies_mhz();
                self.cpu_usage_prev = cpu::read_cpu_usage_snapshot();
                self.cpu_usage_percent = None; // need two snapshots
                if let Some((used, total)) = cpu::read_memory_usage() {
                    self.memory_used_kb = Some(used);
                    self.memory_total_kb = Some(total);
                }

                let new_id = cosmic::iced::window::Id::unique();
                self.popup.replace(new_id);

                let popup_width = self.popup_width() as u32;
                let mut popup_settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap(),
                    new_id,
                    Some((popup_width, 1)),
                    None,
                    None,
                );
                popup_settings.positioner.size_limits = self.popup_limits();

                return cosmic::iced::platform_specific::shell::commands::popup::get_popup(
                    popup_settings,
                );
            }
            Message::PopupClosed(id) => {
                self.popup.take_if(|stored_id| stored_id == &id);
            }
            Message::SetTurbo(enabled) => {
                if let Err(e) = cpu::write_turbo(enabled) {
                    tracing::error!("Failed to set turbo: {e}");
                } else {
                    self.turbo_enabled = Some(enabled);
                    let min_freq = cpu::read_min_frequency_mhz();
                    let max_freq = cpu::read_effective_max_frequency_mhz();
                    self.freq_bounds = min_freq.zip(max_freq);
                    self.scaling_min_mhz = cpu::read_scaling_min_mhz();
                    self.scaling_max_mhz = cpu::read_scaling_max_mhz();
                    if let Some(handler) = &self.config_handler
                        && let Err(error) = self.config.set_turbo_enabled(handler, enabled)
                    {
                        tracing::error!("Failed to save turbo config: {error}");
                    }
                }
            }
            Message::SetGovernor(idx) => {
                if let Some(gov) = self.available_governors.get(idx) {
                    if let Err(e) = cpu::write_governor(gov) {
                        tracing::error!("Failed to set governor: {e}");
                    } else {
                        self.current_governor = Some(gov.clone());
                        if let Some(handler) = &self.config_handler
                            && let Err(error) = self.config.set_saved_governor(handler, gov.clone())
                        {
                            tracing::error!("Failed to save governor config: {error}");
                        }
                    }
                }
            }
            Message::SetEpp(idx) => {
                if let Some(epp) = self.available_epp.get(idx) {
                    if let Err(e) = cpu::write_epp(epp) {
                        tracing::error!("Failed to set EPP: {e}");
                    } else {
                        self.current_epp = Some(epp.clone());
                    }
                }
            }
            Message::PreviewScalingMin(mhz) => {
                self.preview_min_mhz = Some(mhz);
            }
            Message::PreviewScalingMax(mhz) => {
                self.preview_max_mhz = Some(mhz);
            }
            Message::CommitScalingMin => {
                if let Some(mhz) = self.preview_min_mhz.take() {
                    let khz = (mhz * 1000.0) as u64;
                    if let Err(e) = cpu::write_scaling_min_khz(khz) {
                        tracing::error!("Failed to set min freq: {e}");
                    } else {
                        self.scaling_min_mhz = Some(mhz);
                    }
                }
            }
            Message::CommitScalingMax => {
                if let Some(mhz) = self.preview_max_mhz.take() {
                    let khz = (mhz * 1000.0) as u64;
                    if let Err(e) = cpu::write_scaling_max_khz(khz) {
                        tracing::error!("Failed to set max freq: {e}");
                    } else {
                        self.scaling_max_mhz = Some(mhz);
                    }
                }
            }
            Message::UpdateIntervalChanged(value) => {
                self.update_interval_input = value.clone();
                if let Ok(ms) = value.parse::<u64>() {
                    if ms >= 100 {
                        if let Some(handler) = &self.config_handler
                            && let Err(error) = self.config.set_update_interval_ms(handler, ms)
                        {
                            tracing::error!("Failed to save update interval: {error}");
                        }
                    }
                }
            }
            Message::ToggleSettings => {
                self.show_settings = !self.show_settings;
            }
            Message::MainTabActivated(id) => {
                self.main_tab_model.activate(id);
            }
            Message::SetFreqDisplayMode(idx) => {
                let mode = match idx {
                    1 => FreqDisplayMode::Minimum,
                    2 => FreqDisplayMode::Maximum,
                    _ => FreqDisplayMode::Average,
                };
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_freq_display_mode(handler, mode)
                {
                    tracing::error!("Failed to save display mode: {error}");
                }
            }
            Message::SetFreqUnit(idx) => {
                let unit = match idx {
                    1 => FreqUnit::Gigahertz,
                    2 => FreqUnit::Megahertz,
                    _ => FreqUnit::Auto,
                };
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_freq_unit(handler, unit)
                {
                    tracing::error!("Failed to save frequency unit: {error}");
                }
            }
            Message::SetFreqPrecision(idx) => {
                let precision = idx.clamp(0, 3) as u32;
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_freq_precision(handler, precision)
                {
                    tracing::error!("Failed to save frequency precision: {error}");
                }
            }
            Message::SetPerCoreColumns(idx) => {
                let columns = match idx {
                    0 => 2,
                    1 => 3,
                    2 => 4,
                    3 => 6,
                    4 => 8,
                    _ => 4,
                };
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_per_core_columns(handler, columns)
                {
                    tracing::error!("Failed to save per-core columns: {error}");
                }
            }
            Message::SetLayout(idx) => {
                let layout = match idx {
                    1 => PopupLayout::Horizontal,
                    2 => PopupLayout::Vertical,
                    _ => PopupLayout::Tabbed,
                };
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_popup_layout(handler, layout)
                {
                    tracing::error!("Failed to save popup layout: {error}");
                }
            }
            Message::SetShowPanelIcon(enabled) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_show_panel_icon(handler, enabled)
                {
                    tracing::error!("Failed to save panel icon setting: {error}");
                }
            }
            Message::SetShowPanelText(enabled) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_show_panel_text(handler, enabled)
                {
                    tracing::error!("Failed to save panel text setting: {error}");
                }
            }
            Message::SetShowPerCoreFreqs(enabled) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_show_per_core_freqs(handler, enabled)
                {
                    tracing::error!("Failed to save per-core freq setting: {error}");
                }
            }
            Message::SetShowPerCoreUsage(enabled) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_show_per_core_usage(handler, enabled)
                {
                    tracing::error!("Failed to save per-core usage setting: {error}");
                }
            }
            Message::SetShowCpuUsage(enabled) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_show_cpu_usage(handler, enabled)
                {
                    tracing::error!("Failed to save CPU usage setting: {error}");
                }
            }
            Message::SetShowMemoryUsage(enabled) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_show_memory_usage(handler, enabled)
                {
                    tracing::error!("Failed to save memory usage setting: {error}");
                }
            }
            Message::SetShowMinFrequencySlider(enabled) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) =
                        self.config.set_show_min_frequency_slider(handler, enabled)
                {
                    tracing::error!("Failed to save min frequency slider setting: {error}");
                }
            }
            Message::SetShowMaxFrequencySlider(enabled) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) =
                        self.config.set_show_max_frequency_slider(handler, enabled)
                {
                    tracing::error!("Failed to save max frequency slider setting: {error}");
                }
            }
            Message::SetRestoreOnStart(enabled) => {
                if let Some(handler) = &self.config_handler
                    && let Err(error) = self.config.set_restore_settings_on_start(handler, enabled)
                {
                    tracing::error!("Failed to save restore-on-start setting: {error}");
                }
            }
            Message::Surface(action) => {
                return cosmic::task::message(cosmic::Action::Cosmic(
                    cosmic::app::Action::Surface(action),
                ));
            }
        }

        cosmic::task::none()
    }

    fn view(&self) -> cosmic::Element<'_, Message> {
        // Fixed width so the panel button does not resize as the text changes.
        let freq_text = cosmic::widget::container(
            cosmic::widget::text(self.format_freq(self.current_freq_mhz)).size(12),
        )
        .center_x(cosmic::iced::Length::Fixed(PANEL_TEXT_WIDTH));

        let show_icon = self.config.show_panel_icon;
        let show_text = self.config.show_panel_text;

        let content: cosmic::Element<'_, Message> = if show_icon && show_text {
            cosmic::iced_widget::row![
                cosmic::widget::icon::from_name("utilities-system-monitor-symbolic").size(16),
                freq_text,
            ]
            .spacing(6)
            .align_y(cosmic::iced::Alignment::Center)
            .into()
        } else if show_icon {
            cosmic::widget::icon::from_name("utilities-system-monitor-symbolic")
                .size(16)
                .into()
        } else {
            // Fall back to text even when disabled so the button is not empty
            freq_text.into()
        };

        let button = cosmic::widget::button::custom(content)
            .class(cosmic::theme::Button::AppletIcon)
            .on_press_down(Message::ToggleWindow);

        cosmic::widget::autosize::autosize(button, cosmic::widget::Id::unique()).into()
    }

    fn view_window(&self, _id: cosmic::iced::window::Id) -> cosmic::Element<'_, Message> {
        let content = if self.show_settings {
            self.settings_view()
        } else {
            self.main_view()
        };

        let container = cosmic::widget::container(content)
            .padding(cosmic::iced::Padding::ZERO.top(8.0).bottom(8.0));

        self.core
            .applet
            .popup_container(container)
            .limits(self.popup_limits())
            .into()
    }
}

impl CpuFreqApplet {
    /// Main popup, dispatching to the configured layout.
    fn main_view(&self) -> cosmic::Element<'_, Message> {
        match self.config.popup_layout {
            PopupLayout::Tabbed => self.tabbed_view(),
            PopupLayout::Horizontal | PopupLayout::Vertical => self.single_view(),
        }
    }

    /// The settings cog button shown in the top-right of the main popup.
    fn settings_cog(&self) -> cosmic::Element<'_, Message> {
        cosmic::widget::button::icon(cosmic::widget::icon::from_name("emblem-system-symbolic"))
            .on_press(Message::ToggleSettings)
            .padding(4)
            .into()
    }

    /// Tabbed layout: compact header, Controls/Details tabs, active tab body.
    fn tabbed_view(&self) -> cosmic::Element<'_, Message> {
        let mut col = cosmic::iced_widget::column![].spacing(8);

        // Header: current frequency prominent, settings cog on the right
        let freq_col = cosmic::iced_widget::column![
            cosmic::widget::text::caption(fl!("frequency")),
            cosmic::widget::text::title3(self.format_freq(self.current_freq_mhz)),
        ]
        .spacing(2);
        let header = cosmic::iced_widget::row![
            freq_col,
            cosmic::widget::Space::new().width(cosmic::iced::Length::Fill),
            self.settings_cog(),
        ]
        .align_y(cosmic::iced::Alignment::Center);
        col = col.push(cosmic::applet::padded_control(header));

        // Tab switcher
        let tabs = cosmic::widget::segmented_control::horizontal(&self.main_tab_model)
            .on_activate(Message::MainTabActivated);
        col = col.push(cosmic::applet::padded_control(tabs));

        // Active tab body
        match self
            .main_tab_model
            .data::<MainTab>(self.main_tab_model.active())
            .copied()
            .unwrap_or(MainTab::Controls)
        {
            MainTab::Controls => col = col.push(self.build_controls_section(false)),
            MainTab::Details => col = col.push(self.build_details_section(false)),
        }

        col.into()
    }

    /// Single-page layout: compact frequency header, then info + controls.
    /// Horizontal places the two sections side by side; Vertical stacks them.
    fn single_view(&self) -> cosmic::Element<'_, Message> {
        let mut col = cosmic::iced_widget::column![].spacing(8);

        // Header: current frequency, settings cog on the right
        let freq_col = cosmic::iced_widget::column![
            cosmic::widget::text::caption(fl!("frequency")),
            cosmic::widget::text::title3(self.format_freq(self.current_freq_mhz)),
        ]
        .spacing(2);
        let header = cosmic::iced_widget::row![
            freq_col,
            cosmic::widget::Space::new().width(cosmic::iced::Length::Fill),
            self.settings_cog(),
        ]
        .align_y(cosmic::iced::Alignment::Center);
        col = col.push(cosmic::applet::padded_control(header));

        col = col.push(cosmic::applet::padded_control(
            cosmic::widget::divider::horizontal::default(),
        ));

        match self.config.popup_layout {
            PopupLayout::Horizontal => {
                // Compact columns (no inner horizontal padding); the outer inset
                // is applied to each column so the middle gap is only `spacing`.
                let info_section = cosmic::widget::container(self.build_details_section(true))
                    .width(cosmic::iced::Length::FillPortion(1))
                    .padding(cosmic::iced::Padding {
                        top: 0.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 12.0,
                    });
                let controls_section = cosmic::widget::container(self.build_controls_section(true))
                    .width(cosmic::iced::Length::FillPortion(1))
                    .padding(cosmic::iced::Padding {
                        top: 0.0,
                        right: 12.0,
                        bottom: 0.0,
                        left: 0.0,
                    });
                let body = cosmic::iced_widget::row![info_section, controls_section]
                    .spacing(16)
                    .align_y(cosmic::iced::Alignment::Start);
                col = col.push(body);
            }
            _ => {
                col = col.push(
                    cosmic::widget::container(self.build_details_section(false))
                        .width(cosmic::iced::Length::FillPortion(1)),
                );
                col = col.push(cosmic::applet::padded_control(
                    cosmic::widget::divider::horizontal::default(),
                ));
                col = col.push(
                    cosmic::widget::container(self.build_controls_section(false))
                        .width(cosmic::iced::Length::FillPortion(1)),
                );
            }
        }

        col.into()
    }

    /// Settings page with grouped sections.
    fn settings_view(&self) -> cosmic::Element<'_, Message> {
        let mut col = cosmic::iced_widget::column![].spacing(8);

        let header = cosmic::iced_widget::row![
            cosmic::widget::button::icon(cosmic::widget::icon::from_name("go-previous-symbolic"))
                .on_press(Message::ToggleSettings)
                .padding(4),
            cosmic::widget::text::title4(fl!("settings")),
            cosmic::widget::Space::new().width(cosmic::iced::Length::Fill),
        ]
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center);
        col = col.push(cosmic::applet::padded_control(header));

        col = col.push(cosmic::applet::padded_control(
            cosmic::widget::divider::horizontal::default(),
        ));

        let body = cosmic::widget::scrollable(self.settings_sections())
            .height(cosmic::iced::Length::Fixed(500.0));
        col = col.push(cosmic::applet::padded_control(body));

        col.into()
    }

    fn settings_sections(&self) -> cosmic::Element<'_, Message> {
        use cosmic::widget::settings;

        let display_mode_idx = match self.config.freq_display_mode {
            FreqDisplayMode::Average => Some(0),
            FreqDisplayMode::Minimum => Some(1),
            FreqDisplayMode::Maximum => Some(2),
        };
        let freq_unit_idx = match self.config.freq_unit {
            FreqUnit::Auto => Some(0),
            FreqUnit::Gigahertz => Some(1),
            FreqUnit::Megahertz => Some(2),
        };
        let precision_idx = Some(self.config.freq_precision.clamp(0, 3) as usize);
        let layout_idx = match self.config.popup_layout {
            PopupLayout::Tabbed => Some(0),
            PopupLayout::Horizontal => Some(1),
            PopupLayout::Vertical => Some(2),
        };
        let columns_idx = match self.config.per_core_columns {
            2 => Some(0),
            3 => Some(1),
            6 => Some(3),
            8 => Some(4),
            _ => Some(2),
        };

        let panel = settings::section()
            .title(fl!("panel-section"))
            .add(
                settings::item::builder(fl!("show-panel-icon"))
                    .toggler(self.config.show_panel_icon, Message::SetShowPanelIcon),
            )
            .add(
                settings::item::builder(fl!("show-panel-text"))
                    .toggler(self.config.show_panel_text, Message::SetShowPanelText),
            )
            .add(
                settings::item::builder(fl!("display-mode")).control(self.popup_dropdown(
                    &self.display_mode_options,
                    display_mode_idx,
                    Message::SetFreqDisplayMode,
                )),
            )
            .add(
                settings::item::builder(fl!("frequency-unit")).control(self.popup_dropdown(
                    &self.freq_unit_options,
                    freq_unit_idx,
                    Message::SetFreqUnit,
                )),
            )
            .add(
                settings::item::builder(fl!("precision")).control(self.popup_dropdown(
                    &self.precision_options,
                    precision_idx,
                    Message::SetFreqPrecision,
                )),
            )
            .add(
                settings::item::builder(fl!("popup-layout")).control(self.popup_dropdown(
                    &self.layout_options,
                    layout_idx,
                    Message::SetLayout,
                )),
            );

        let details = settings::section()
            .title(fl!("details-section"))
            .add(settings::item::builder(fl!("show-per-core-freqs")).toggler(
                self.config.show_per_core_freqs,
                Message::SetShowPerCoreFreqs,
            ))
            .add(settings::item::builder(fl!("per-core-columns")).control(
                self.popup_dropdown(
                    &self.columns_options,
                    columns_idx,
                    Message::SetPerCoreColumns,
                ),
            ))
            .add(settings::item::builder(fl!("show-per-core-usage")).toggler(
                self.config.show_per_core_usage,
                Message::SetShowPerCoreUsage,
            ))
            .add(
                settings::item::builder(fl!("show-cpu-usage"))
                    .toggler(self.config.show_cpu_usage, Message::SetShowCpuUsage),
            )
            .add(
                settings::item::builder(fl!("show-memory-usage"))
                    .toggler(self.config.show_memory_usage, Message::SetShowMemoryUsage),
            );

        let controls = settings::section()
            .title(fl!("controls-section"))
            .add(
                settings::item::builder(fl!("show-min-frequency-slider")).toggler(
                    self.config.show_min_frequency_slider,
                    Message::SetShowMinFrequencySlider,
                ),
            )
            .add(
                settings::item::builder(fl!("show-max-frequency-slider")).toggler(
                    self.config.show_max_frequency_slider,
                    Message::SetShowMaxFrequencySlider,
                ),
            );

        let behavior = settings::section()
            .title(fl!("behavior-section"))
            .add(settings::item::builder(fl!("restore-on-start")).toggler(
                self.config.restore_settings_on_start,
                Message::SetRestoreOnStart,
            ))
            .add(
                settings::item::builder(fl!("update-interval")).control(
                    cosmic::widget::text_input("2000", &self.update_interval_input)
                        .on_input(Message::UpdateIntervalChanged),
                ),
            );

        let system = settings::section()
            .title(fl!("system-section"))
            .add(self.system_info_element());

        settings::view_column(vec![
            panel.into(),
            details.into(),
            controls.into(),
            behavior.into(),
            system.into(),
        ])
        .into()
    }

    fn popup_width(&self) -> f32 {
        match self.config.popup_layout {
            PopupLayout::Horizontal => HORIZONTAL_POPUP_WIDTH,
            _ => DEFAULT_POPUP_WIDTH,
        }
    }

    fn popup_limits(&self) -> cosmic::iced::Limits {
        let width = self.popup_width();
        cosmic::iced::Limits::NONE
            .min_height(1.0)
            .min_width(width)
            .max_width(width)
            .max_height(1000.0)
    }

    /// Format a frequency value according to the configured unit and precision.
    fn format_freq(&self, mhz: Option<f64>) -> String {
        match self.config.freq_unit {
            FreqUnit::Megahertz => mhz
                .map(|m| format!("{:.0} MHz", m))
                .unwrap_or_else(|| "--".to_string()),
            FreqUnit::Gigahertz => mhz
                .map(|m| Self::format_ghz(m, self.config.freq_precision))
                .unwrap_or_else(|| "--".to_string()),
            FreqUnit::Auto => match mhz {
                Some(m) if m >= 1000.0 => Self::format_ghz(m, self.config.freq_precision),
                Some(m) => format!("{:.0} MHz", m),
                None => "--".to_string(),
            },
        }
    }

    fn format_ghz(mhz: f64, precision: u32) -> String {
        let prec = precision.min(3) as usize;
        format!("{:.prec$} GHz", mhz / 1000.0, prec = prec)
    }

    /// Restore saved turbo setting if system state differs from config.
    fn restore_turbo_setting(&mut self) {
        if let Some(system_turbo) = self.turbo_enabled {
            if system_turbo != self.config.turbo_enabled {
                tracing::info!(
                    "Restoring turbo setting: {} (system was: {})",
                    self.config.turbo_enabled,
                    system_turbo
                );
                if cpu::write_turbo(self.config.turbo_enabled).is_ok() {
                    self.turbo_enabled = Some(self.config.turbo_enabled);
                }
            }
        }
    }

    /// Restore saved governor if the system state differs from config.
    fn restore_governor_setting(&mut self) {
        if self.config.saved_governor.is_empty() {
            return;
        }
        if self
            .available_governors
            .iter()
            .any(|g| g == &self.config.saved_governor)
            && self.current_governor.as_deref() != Some(self.config.saved_governor.as_str())
        {
            tracing::info!(
                "Restoring governor: {} (system was: {:?})",
                self.config.saved_governor,
                self.current_governor
            );
            if cpu::write_governor(&self.config.saved_governor).is_ok() {
                self.current_governor = Some(self.config.saved_governor.clone());
            }
        }
    }

    /// Rebuild pretty display labels for governor and EPP dropdowns.
    fn rebuild_labels(&mut self) {
        self.governor_labels = self
            .available_governors
            .iter()
            .map(|s| prettify_label(s))
            .collect();
        self.epp_labels = self
            .available_epp
            .iter()
            .map(|s| prettify_label(s))
            .collect();
    }

    /// System information block (CPU model, machine, OS, kernel).
    fn system_info_element(&self) -> cosmic::Element<'_, Message> {
        let mut info_col = cosmic::iced_widget::column![].spacing(2);
        if let Some(model) = &self.cpu_model {
            info_col = info_col.push(cosmic::widget::text::body(model.clone()));
        }
        if let (Some(vendor), Some(model)) = (&self.machine_vendor, &self.machine_model) {
            info_col = info_col.push(cosmic::widget::text::caption(format!("{vendor} {model}")));
        }
        if let Some(os) = &self.os_name {
            info_col = info_col.push(cosmic::widget::text::caption(os.clone()));
        }
        if let Some(kern) = &self.kernel_version {
            info_col = info_col.push(cosmic::widget::text::caption(format!(
                "{} {kern}",
                fl!("kernel")
            )));
        }
        info_col.into()
    }

    /// Live info blocks (per-core grid, CPU usage, memory usage) as elements.
    fn info_block_elements(&self) -> Vec<cosmic::Element<'_, Message>> {
        let mut elements = Vec::new();

        // Per-core frequency grid
        if self.config.show_per_core_freqs && !self.per_core_freqs.is_empty() {
            let cols = self.config.per_core_columns.clamp(1, 16) as usize;
            let mut grid_col = cosmic::iced_widget::column![].spacing(6);

            for (chunk_idx, chunk) in self.per_core_freqs.chunks(cols).enumerate() {
                let mut row = cosmic::iced_widget::row![].spacing(8);
                for (i, (id, mhz)) in chunk.iter().enumerate() {
                    let freq_str = self.format_freq(Some(*mhz));
                    let mut cell = cosmic::iced_widget::column![
                        cosmic::widget::text(format!("CPU{id}")).size(11),
                        cosmic::widget::text(freq_str).size(13),
                    ]
                    .align_x(cosmic::iced::Alignment::Center)
                    .width(cosmic::iced::Length::FillPortion(1));

                    if self.config.show_per_core_usage {
                        let core_idx = chunk_idx * cols + i;
                        let usage_str = self
                            .per_core_usage
                            .get(core_idx)
                            .map(|pct| format!("{pct:.0}%"))
                            .unwrap_or_else(|| "--".to_string());
                        cell = cell.push(cosmic::widget::text(usage_str).size(10));
                    }

                    row = row.push(cell);
                }
                for _ in chunk.len()..cols {
                    row = row.push(
                        cosmic::widget::Space::new().width(cosmic::iced::Length::FillPortion(1)),
                    );
                }
                grid_col = grid_col.push(row);
            }
            elements.push(grid_col.into());
        }

        // CPU usage bar
        if self.config.show_cpu_usage {
            let pct = self.cpu_usage_percent.unwrap_or(0.0);
            let bar_row = cosmic::iced_widget::column![
                cosmic::iced_widget::row![
                    cosmic::widget::text::body(fl!("cpu-usage")),
                    cosmic::widget::Space::new().width(cosmic::iced::Length::Fill),
                    cosmic::widget::text::body(format!("{pct:.1}%")),
                ]
                .align_y(cosmic::iced::Alignment::Center),
                cosmic::widget::progress_bar(0.0..=100.0, pct as f32),
            ]
            .spacing(4);
            elements.push(bar_row.into());
        }

        // Memory usage bar
        if self.config.show_memory_usage {
            let (pct, label) = match (self.memory_used_kb, self.memory_total_kb) {
                (Some(used), Some(total)) if total > 0 => {
                    let p = (used as f64 / total as f64) * 100.0;
                    (p, format!("{p:.1}%"))
                }
                _ => (0.0, "--".to_string()),
            };
            let bar_row = cosmic::iced_widget::column![
                cosmic::iced_widget::row![
                    cosmic::widget::text::body(fl!("memory-usage")),
                    cosmic::widget::Space::new().width(cosmic::iced::Length::Fill),
                    cosmic::widget::text::body(label),
                ]
                .align_y(cosmic::iced::Alignment::Center),
                cosmic::widget::progress_bar(0.0..=100.0, pct as f32),
            ]
            .spacing(4);
            elements.push(bar_row.into());
        }

        elements
    }

    /// Build the Details tab (per-core grid, usage bars).
    ///
    /// `compact` drops the horizontal row padding so the caller can supply the
    /// outer inset (used when two sections sit side by side).
    fn build_details_section(&self, compact: bool) -> cosmic::Element<'_, Message> {
        let mut col = cosmic::iced_widget::column![].spacing(8);

        for element in self.info_block_elements() {
            col = col.push(padded_row(element, compact));
        }

        col.into()
    }

    /// A dropdown that renders as a native popup surface (avoids the in-window
    /// menu overflowing the narrow applet popup).
    fn popup_dropdown<'a, S>(
        &self,
        selections: &'a [S],
        selected: Option<usize>,
        on_selected: impl Fn(usize) -> Message + Send + Sync + 'static,
    ) -> cosmic::widget::dropdown::Dropdown<'a, S, Message, Message>
    where
        S: AsRef<str> + Clone + Send + Sync + 'static,
    {
        cosmic::widget::dropdown::popup_dropdown(
            selections,
            selected,
            on_selected,
            self.popup.unwrap_or(cosmic::iced::window::Id::NONE),
            Message::Surface,
            |m| m,
        )
    }

    /// Build the Controls tab (turbo, governor, EPP, freq sliders).
    ///
    /// `compact` drops the horizontal row padding so the caller can supply the
    /// outer inset (used when two sections sit side by side).
    fn build_controls_section(&self, compact: bool) -> cosmic::Element<'_, Message> {
        use cosmic::widget::settings;

        let mut col = cosmic::iced_widget::column![].spacing(8);

        // Turbo boost toggle
        col = col.push(padded_row(
            settings::item::builder(fl!("turbo-boost"))
                .toggler(self.turbo_enabled.unwrap_or(false), Message::SetTurbo)
                .into(),
            compact,
        ));

        // Governor dropdown (pretty labels)
        if !self.available_governors.is_empty() {
            let current_idx = self
                .current_governor
                .as_ref()
                .and_then(|g| self.available_governors.iter().position(|x| x == g));

            col = col.push(padded_row(
                settings::item::builder(fl!("cpu-governor"))
                    .control(self.popup_dropdown(
                        &self.governor_labels,
                        current_idx,
                        Message::SetGovernor,
                    ))
                    .into(),
                compact,
            ));
        }

        // EPP dropdown (pretty labels)
        if !self.available_epp.is_empty() {
            let current_idx = self
                .current_epp
                .as_ref()
                .and_then(|e| self.available_epp.iter().position(|x| x == e));

            col = col.push(padded_row(
                settings::item::builder(fl!("energy-preference"))
                    .control(self.popup_dropdown(&self.epp_labels, current_idx, Message::SetEpp))
                    .into(),
                compact,
            ));
        }

        // Frequency sliders
        if let Some((min_bound, max_bound)) = self.freq_bounds {
            let step = 100.0;
            let range = max_bound - min_bound;
            let percent = |v: f64| {
                if range > 0.0 {
                    ((v - min_bound) / range * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                }
            };

            let display_min = self.preview_min_mhz.or(self.scaling_min_mhz);
            let display_max = self.preview_max_mhz.or(self.scaling_max_mhz);

            if self.config.show_min_frequency_slider
                && let Some(scaling_min) = display_min
            {
                let min_row = cosmic::iced_widget::column![
                    cosmic::iced_widget::row![
                        cosmic::widget::text::body(format!(
                            "{}: {}",
                            fl!("min-frequency"),
                            self.format_freq(Some(scaling_min))
                        )),
                        cosmic::widget::Space::new().width(cosmic::iced::Length::Fill),
                        cosmic::widget::text::body(format!("{:.0}%", percent(scaling_min))),
                    ]
                    .align_y(cosmic::iced::Alignment::Center),
                    cosmic::widget::slider(min_bound..=max_bound, scaling_min, move |v| {
                        Message::PreviewScalingMin((v / step).round() * step)
                    })
                    .on_release(Message::CommitScalingMin),
                ]
                .spacing(4);
                col = col.push(padded_row(min_row.into(), compact));
            }

            if self.config.show_max_frequency_slider
                && let Some(scaling_max) = display_max
            {
                let max_row = cosmic::iced_widget::column![
                    cosmic::iced_widget::row![
                        cosmic::widget::text::body(format!(
                            "{}: {}",
                            fl!("max-frequency"),
                            self.format_freq(Some(scaling_max))
                        )),
                        cosmic::widget::Space::new().width(cosmic::iced::Length::Fill),
                        cosmic::widget::text::body(format!("{:.0}%", percent(scaling_max))),
                    ]
                    .align_y(cosmic::iced::Alignment::Center),
                    cosmic::widget::slider(min_bound..=max_bound, scaling_max, move |v| {
                        Message::PreviewScalingMax((v / step).round() * step)
                    })
                    .on_release(Message::CommitScalingMax),
                ]
                .spacing(4);
                col = col.push(padded_row(max_row.into(), compact));
            }
        }

        col.into()
    }
}

/// Wrap a row in the standard menu padding, or vertical-only padding when
/// `compact` (so a caller can supply the horizontal inset for side-by-side
/// sections).
fn padded_row<'a>(
    widget: cosmic::Element<'a, Message>,
    compact: bool,
) -> cosmic::Element<'a, Message> {
    if compact {
        cosmic::widget::container(widget)
            .padding([4.0, 0.0])
            .width(cosmic::iced::Length::Fill)
            .into()
    } else {
        cosmic::applet::padded_control(widget).into()
    }
}

/// Convert a sysfs-style label like "balance_performance" to "Balance Performance".
fn prettify_label(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{upper}{}", chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
