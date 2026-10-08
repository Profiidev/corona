use std::{
  collections::BTreeMap,
  path::{Path, PathBuf},
  time::Duration,
};

use anyhow::Result;
use gpui_kit::{App, Global};
use serde::{Deserialize, Serialize};

use crate::bar::BarConfig;

pub mod bar;
pub mod placement;
mod read;
mod watch;
mod write;

pub use read::{Loaded, config_dir, config_files, read, read_files, settings_file};
pub use watch::watch;
pub use write::update;

pub const APP_NAME: &str = "corona";

/// Reads the settings and makes them the [`Config`] global. When they do not
/// parse the defaults stand in, so the shell still starts, and the error is
/// returned to be shown.
pub fn load(cx: &mut App) -> Result<()> {
  match read() {
    Ok(loaded) => {
      loaded.warn_unknown();
      cx.set_global(loaded.config);
      Ok(())
    }
    Err(e) => {
      cx.set_global(Config::default());
      Err(e)
    }
  }
}

/// Every setting. Each section defaults on its own, so a file only lists what it changes.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct Config {
  pub shell: ShellConfig,
  pub theme: ThemeConfig,
  pub wallpaper: WallpaperConfig,
  /// Bars by name; every one of them opens on every monitor
  pub bar: BTreeMap<String, BarConfig>,
  pub notification: NotificationConfig,
  pub osd: OsdConfig,
  pub lockscreen: LockscreenConfig,
  pub screenshot: ScreenshotConfig,
  pub window_switcher: WindowSwitcherConfig,
  pub taskbar: TaskbarConfig,
  pub control_center: ControlCenterConfig,
  pub weather: WeatherConfig,
  pub location: LocationConfig,
  pub brightness: BrightnessConfig,
  pub system: SystemConfig,
  pub idle: IdleConfig,
}

impl Default for Config {
  fn default() -> Self {
    Self {
      shell: ShellConfig::default(),
      theme: ThemeConfig::default(),
      wallpaper: WallpaperConfig::default(),
      bar: BTreeMap::from([("main".to_string(), BarConfig::default())]),
      notification: NotificationConfig::default(),
      osd: OsdConfig::default(),
      lockscreen: LockscreenConfig::default(),
      screenshot: ScreenshotConfig::default(),
      window_switcher: WindowSwitcherConfig::default(),
      taskbar: TaskbarConfig::default(),
      control_center: ControlCenterConfig::default(),
      weather: WeatherConfig::default(),
      location: LocationConfig::default(),
      brightness: BrightnessConfig::default(),
      system: SystemConfig::default(),
      idle: IdleConfig::default(),
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct ShellConfig {
  /// Path, `~/` path or http(s) URL of the picture on the dashboard
  pub avatar: Option<String>,
  /// Unset: `~/.config/corona/plugins`; may start with `~/`. Read at startup only
  pub plugin_dir: Option<PathBuf>,
  /// Language of the interface, like `de`; unset follows `LC_MESSAGES`/`LANG`
  pub language: Option<String>,
  pub animation: AnimationConfig,
  pub privacy: PrivacyConfig,
}

/// Apps the privacy widget, its log and OSD leave out: case-insensitive
/// regexes against the app's name, empty for none
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct PrivacyConfig {
  pub mic_filter_regex: String,
  pub cam_filter_regex: String,
  pub screen_filter_regex: String,
}

/// `path` with a leading `~/` in the home directory
pub fn expand_home(path: &Path) -> PathBuf {
  match (path.strip_prefix("~"), dirs::home_dir()) {
    (Ok(rest), Some(home)) => home.join(rest),
    _ => path.to_path_buf(),
  }
}

impl ShellConfig {
  pub fn plugin_dir(&self) -> PathBuf {
    self
      .plugin_dir
      .as_deref()
      .map(expand_home)
      .unwrap_or_else(|| {
        dirs::config_dir()
          .unwrap_or_else(|| PathBuf::from("."))
          .join("corona/plugins")
      })
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct AnimationConfig {
  pub enabled: bool,
  /// Multiplier on animation speed: 2 is twice as fast
  pub speed: f32,
}

impl AnimationConfig {
  /// `base` at this speed, zero when animations are off. Slower than a
  /// hundredth is a hundredth.
  pub fn duration(&self, base: Duration) -> Duration {
    match self.enabled && self.speed > 0. {
      true => Duration::try_from_secs_f64(base.as_secs_f64() / f64::from(self.speed).max(0.01))
        .unwrap_or(Duration::MAX),
      false => Duration::ZERO,
    }
  }
}

impl Default for AnimationConfig {
  fn default() -> Self {
    Self {
      enabled: true,
      speed: 1.,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct ThemeConfig {
  /// A theme's name, its `Dark`/`Light` counterpart serves the other mode
  pub name: String,
  /// Unset: the mode of the named theme
  pub mode: Option<ThemeMode>,
  /// Unset: the theme's font
  pub font_family: Option<String>,
  /// Text and the spacing measured in it
  pub font_scale: f32,
  /// 0 for square corners
  pub corner_radius_scale: f32,
  /// Shadows under popovers
  pub shadow: bool,
  /// Borders around OSDs, tooltips, menus and the window switcher
  pub popup_borders: bool,
  /// Borders around the cards in panels
  pub card_borders: bool,
  /// Round the screen corners no bar rounds
  pub screen_corners: bool,
}

impl ThemeConfig {
  /// `width` when popup borders are on, else none
  pub fn popup_border(&self, width: f32) -> f32 {
    if self.popup_borders { width } else { 0. }
  }

  /// `color` when popup borders are on, else transparent. For popups whose
  /// layout counts the border in, so its space stays.
  pub fn popup_border_color(&self, color: gpui_kit::Hsla) -> gpui_kit::Hsla {
    if self.popup_borders {
      color
    } else {
      gpui_kit::transparent_black()
    }
  }
}

impl Default for ThemeConfig {
  fn default() -> Self {
    Self {
      name: "shadcn Zinc Blue Dark".to_string(),
      mode: None,
      font_family: None,
      font_scale: 1.,
      corner_radius_scale: 1.,
      shadow: true,
      popup_borders: true,
      card_borders: true,
      screen_corners: true,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
  Dark,
  Light,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct WallpaperConfig {
  /// Path, `~/` path or http(s) URL; unset: no wallpaper
  pub path: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum NotificationPosition {
  TopLeft,
  #[default]
  TopRight,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct NotificationConfig {
  /// Run the notification daemon; read at startup only
  pub enabled: bool,
  pub position: NotificationPosition,
  pub width: f32,
  /// Distance from the screen edges
  pub offset: f32,
  pub timeout_ms: u64,
  pub critical_timeout_ms: u64,
  pub background_opacity: f32,
  /// Case-insensitive regex against app name, summary and body; a match is dropped
  pub filter_regex: String,
}

impl Default for NotificationConfig {
  fn default() -> Self {
    Self {
      enabled: true,
      position: NotificationPosition::default(),
      width: 400.,
      offset: 20.,
      timeout_ms: 5000,
      critical_timeout_ms: 10000,
      background_opacity: 1.,
      filter_regex: String::new(),
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct OsdConfig {
  pub enabled: bool,
  pub hide_delay_ms: u64,
  pub position: OsdPosition,
  /// Distance from the screen edge
  pub offset: f32,
  pub background_opacity: f32,
  pub kinds: OsdKinds,
}

impl Default for OsdConfig {
  fn default() -> Self {
    Self {
      enabled: true,
      hide_delay_ms: 1500,
      position: OsdPosition::default(),
      offset: 40.,
      background_opacity: 1.,
      kinds: OsdKinds::default(),
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OsdPosition {
  TopCenter,
  #[default]
  BottomCenter,
  CenterLeft,
  CenterRight,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct OsdKinds {
  pub volume: bool,
  pub brightness: bool,
  pub wifi: bool,
  pub bluetooth: bool,
  pub dnd: bool,
  pub power_profile: bool,
  pub privacy: bool,
  pub keyboard_layout: bool,
  pub lock_keys: bool,
}

impl Default for OsdKinds {
  fn default() -> Self {
    Self {
      volume: true,
      brightness: true,
      wifi: true,
      bluetooth: true,
      dnd: true,
      power_profile: true,
      privacy: true,
      keyboard_layout: true,
      lock_keys: true,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct LockscreenConfig {
  pub lock_before_suspend: bool,
  /// Blur of the captured screen behind the lock, 0 for none
  pub blur: f32,
}

impl Default for LockscreenConfig {
  fn default() -> Self {
    Self {
      lock_before_suspend: true,
      blur: 3.,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct ScreenshotConfig {
  /// Unset: `Screenshots` in the pictures directory; may start with `~/`
  pub directory: Option<PathBuf>,
  /// strftime pattern
  pub filename_pattern: String,
}

impl Default for ScreenshotConfig {
  fn default() -> Self {
    Self {
      directory: None,
      filename_pattern: "screenshot-%Y-%m-%d-%H%M%S-%3f.png".to_string(),
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct WindowSwitcherConfig {
  /// Only show what is on the focused monitor, even without `--current-monitor`
  pub current_monitor_only: bool,
  pub card_height: f32,
  /// How much the screen behind darkens
  pub backdrop_opacity: f32,
}

impl Default for WindowSwitcherConfig {
  fn default() -> Self {
    Self {
      current_monitor_only: false,
      card_height: 180.,
      backdrop_opacity: 0.4,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct TaskbarConfig {
  pub enabled: bool,
  pub icon_size: f32,
  pub previews: bool,
  pub preview_max_windows: usize,
  pub background_opacity: f32,
}

impl Default for TaskbarConfig {
  fn default() -> Self {
    Self {
      enabled: true,
      icon_size: 36.,
      previews: true,
      preview_max_windows: 5,
      background_opacity: 1.,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
  #[default]
  Monday,
  Sunday,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct ControlCenterConfig {
  /// strftime patterns of the dashboard's clock
  pub time_format: String,
  pub date_format: String,
  pub week_start: Weekday,
  /// Of the panels
  pub background_opacity: f32,
}

impl Default for ControlCenterConfig {
  fn default() -> Self {
    Self {
      time_format: "%H:%M".to_string(),
      date_format: "%a, %d.%m.%Y".to_string(),
      week_start: Weekday::default(),
      background_opacity: 1.,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Units {
  #[default]
  Metric,
  Imperial,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct WeatherConfig {
  pub units: Units,
  pub refresh_minutes: u64,
}

impl Default for WeatherConfig {
  fn default() -> Self {
    Self {
      units: Units::Metric,
      refresh_minutes: 30,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct LocationConfig {
  pub city: Option<String>,
  pub latitude: Option<f64>,
  pub longitude: Option<f64>,
  /// Locate by IP when no city or coordinates are set
  pub auto_locate: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct BrightnessConfig {
  /// Read at startup only
  pub enable_ddcutil: bool,
  pub poll_seconds: u64,
}

impl Default for BrightnessConfig {
  fn default() -> Self {
    Self {
      enable_ddcutil: true,
      poll_seconds: 5,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct IdleConfig {
  /// What happens after a while without input, by name. The compositor waits
  /// while an app inhibits idling, a playing video for one.
  pub behavior: BTreeMap<String, IdleBehavior>,
}

/// Noctalia's timeouts: lock, monitors off, then suspend and later hibernate
impl Default for IdleConfig {
  fn default() -> Self {
    let after = |action, timeout| IdleBehavior {
      action,
      timeout,
      ..Default::default()
    };
    Self {
      behavior: BTreeMap::from([
        ("lock".to_string(), after(IdleAction::Lock, 600.)),
        ("screen-off".to_string(), after(IdleAction::ScreenOff, 660.)),
        (
          "lock-and-suspend".to_string(),
          after(IdleAction::LockAndSuspendThenHibernate, 900.),
        ),
      ]),
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct IdleBehavior {
  pub enabled: bool,
  pub action: IdleAction,
  /// Seconds without input, 0 for never
  pub timeout: f64,
  /// For `command`: a shell command run on idle
  pub command: String,
  /// For `command`: a shell command run when input follows
  pub resume_command: String,
}

impl Default for IdleBehavior {
  fn default() -> Self {
    Self {
      enabled: true,
      action: IdleAction::default(),
      timeout: 0.,
      command: String::new(),
      resume_command: String::new(),
    }
  }
}

impl IdleBehavior {
  /// How long without input until it runs, `None` when it never does
  pub fn after(&self) -> Option<std::time::Duration> {
    let timeout = std::time::Duration::try_from_secs_f64(self.timeout).ok()?;
    (self.enabled && !timeout.is_zero()).then_some(timeout)
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IdleAction {
  Lock,
  /// Monitors off, on again with input
  ScreenOff,
  /// Suspend; still locks first with `lockscreen.lock_before_suspend`
  Suspend,
  /// Lock, then suspend once the lock shows
  LockAndSuspend,
  /// Lock, then suspend and hibernate after logind's `HibernateDelaySec`
  LockAndSuspendThenHibernate,
  #[default]
  Command,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(default)]
pub struct SystemConfig {
  pub monitor: MonitorConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct MonitorConfig {
  pub poll_seconds: u64,
}

impl Default for MonitorConfig {
  fn default() -> Self {
    Self { poll_seconds: 3 }
  }
}

pub trait ConfigProvider {
  fn config(&self) -> &Config;
}

impl ConfigProvider for App {
  fn config(&self) -> &Config {
    self.global::<Config>()
  }
}

impl Global for Config {}

impl Config {
  /// As TOML, every setting written out
  pub fn to_toml(&self) -> Result<String> {
    Ok(toml::to_string_pretty(self)?)
  }
}

/// Calls `on_change` whenever the section `select` picks changes, as the files
/// change or [`update`] writes.
pub fn observe_section<T: Clone + PartialEq + 'static>(
  cx: &mut App,
  select: fn(&Config) -> &T,
  mut on_change: impl FnMut(&T, &mut App) + 'static,
) {
  let mut last = select(cx.config()).clone();
  cx.observe_global::<Config>(move |cx| {
    let now = select(cx.config());
    if *now != last {
      last = now.clone();
      on_change(&last, cx);
    }
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use std::{
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
  };

  use gpui_kit::{self as gpui, TestAppContext};
  use toml::Value;

  use super::*;

  /// Every key path in `value`, like `osd.kinds.volume`
  fn keys(value: &Value, prefix: &str, out: &mut Vec<String>) {
    if let Value::Table(table) = value {
      for (key, value) in table {
        let path = format!("{prefix}{key}");
        keys(value, &format!("{path}."), out);
        out.push(path);
      }
    }
  }

  /// The example lists every setting at its default, so it cannot go stale.
  #[test]
  fn example_is_the_defaults() {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/config.example.toml");
    let text = std::fs::read_to_string(example).unwrap();
    let mut unknown = Vec::new();
    let config: Config =
      serde_ignored::deserialize(toml::Deserializer::parse(&text).unwrap(), |k| {
        unknown.push(k.to_string())
      })
      .unwrap();
    assert_eq!(config, Config::default());
    assert!(unknown.is_empty(), "{unknown:?}");

    let written: Value = toml::from_str(&text).unwrap();
    let (mut want, mut have) = (Vec::new(), Vec::new());
    keys(&Value::try_from(Config::default()).unwrap(), "", &mut want);
    keys(&written, "", &mut have);
    let missing: Vec<_> = want.iter().filter(|k| !have.contains(k)).collect();
    assert!(
      missing.is_empty(),
      "missing from config.example.toml: {missing:?}"
    );
  }

  #[test]
  fn idle_actions_parse() {
    use crate::{IdleAction, IdleBehavior};
    let behavior: IdleBehavior =
      toml::from_str(r#"action = "lock_and_suspend_then_hibernate""#).unwrap();
    assert_eq!(behavior.action, IdleAction::LockAndSuspendThenHibernate);
    assert!(behavior.enabled);
  }

  #[test]
  fn language_parses() {
    use crate::ShellConfig;
    let shell: ShellConfig = toml::from_str("").unwrap();
    assert_eq!(shell.language, None);
    let shell: ShellConfig = toml::from_str(r#"language = "de""#).unwrap();
    assert_eq!(shell.language.as_deref(), Some("de"));
    assert!(
      toml::to_string(&shell)
        .unwrap()
        .starts_with(r#"language = "de""#)
    );
  }

  #[test]
  fn expand_home_only_expands_a_tilde_component() {
    let home = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", home.path()) };
    assert_eq!(expand_home(Path::new("~/x/y")), home.path().join("x/y"));
    assert_eq!(expand_home(Path::new("~")), home.path());
    for unchanged in ["~foo/x", "/abs/~/x", "rel/x", ""] {
      assert_eq!(expand_home(Path::new(unchanged)), PathBuf::from(unchanged));
    }
  }

  #[test]
  fn plugin_dir_expands_or_defaults() {
    let home = tempfile::tempdir().unwrap();
    let xdg = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("HOME", home.path());
      std::env::set_var("XDG_CONFIG_HOME", xdg.path());
    }
    let mut shell = ShellConfig::default();
    assert_eq!(shell.plugin_dir(), xdg.path().join("corona/plugins"));
    shell.plugin_dir = Some("~/plugins".into());
    assert_eq!(shell.plugin_dir(), home.path().join("plugins"));
    shell.plugin_dir = Some("/opt/p".into());
    assert_eq!(shell.plugin_dir(), PathBuf::from("/opt/p"));
  }

  #[test]
  fn animation_duration() {
    let base = Duration::from_millis(200);
    let anim = |enabled, speed| AnimationConfig { enabled, speed };
    assert_eq!(AnimationConfig::default().duration(base), base);
    assert_eq!(anim(true, 2.).duration(base), base / 2);
    assert_eq!(anim(true, 0.5).duration(base), base * 2);
    assert_eq!(anim(true, f32::INFINITY).duration(base), Duration::ZERO);
    for speed in [0., -0., -1., f32::NAN, f32::NEG_INFINITY] {
      assert_eq!(anim(true, speed).duration(base), Duration::ZERO, "{speed}");
    }
    assert_eq!(anim(false, 1.).duration(base), Duration::ZERO);
    assert_eq!(anim(true, 1.).duration(Duration::ZERO), Duration::ZERO);
  }

  #[test]
  fn tiny_animation_speed_is_clamped() {
    let anim = AnimationConfig {
      enabled: true,
      speed: 1e-30,
    };
    let result = std::panic::catch_unwind(|| anim.duration(Duration::from_millis(200)));
    assert_eq!(result.expect("duration panicked"), Duration::from_secs(20));
    assert_eq!(anim.duration(Duration::MAX), Duration::MAX);
  }

  #[test]
  fn popup_border_follows_the_toggle() {
    let red = gpui_kit::red();
    let mut theme = ThemeConfig::default();
    assert_eq!(theme.popup_border(2.), 2.);
    assert_eq!(theme.popup_border_color(red), red);
    theme.popup_borders = false;
    assert_eq!(theme.popup_border(2.), 0.);
    assert_eq!(theme.popup_border_color(red).a, 0.);
  }

  #[test]
  fn idle_after() {
    let behavior = |enabled, timeout| IdleBehavior {
      enabled,
      timeout,
      ..Default::default()
    };
    assert_eq!(
      behavior(true, 1.5).after(),
      Some(Duration::from_millis(1500))
    );
    assert_eq!(behavior(false, 60.).after(), None);
    for timeout in [0., -0., -1., f64::NAN, f64::INFINITY, f64::MAX] {
      assert_eq!(behavior(true, timeout).after(), None, "{timeout}");
    }
    // the defaults all fire, in order
    let defaults = IdleConfig::default().behavior;
    let after = |name: &str| defaults[name].after().unwrap();
    assert!(after("lock") < after("screen-off"));
    assert!(after("screen-off") < after("lock-and-suspend"));
  }

  #[test]
  fn to_toml_round_trips() {
    let text = Config::default().to_toml().unwrap();
    assert_eq!(toml::from_str::<Config>(&text).unwrap(), Config::default());
    // an empty file is the defaults too
    assert_eq!(toml::from_str::<Config>("").unwrap(), Config::default());
  }

  /// The name `value` is written as
  fn spelled<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
      .unwrap()
      .as_str()
      .unwrap()
      .to_string()
  }

  /// `value` written, then read back
  fn round_trip<T: Serialize + serde::de::DeserializeOwned>(value: T) -> T {
    serde_json::from_value(serde_json::to_value(value).unwrap()).unwrap()
  }

  #[test]
  fn enum_spellings() {
    use crate::placement::Placement;
    let cases = [
      (spelled(Placement::Top), "top"),
      (spelled(Placement::Bottom), "bottom"),
      (spelled(Placement::Left), "left"),
      (spelled(Placement::Right), "right"),
      (spelled(ThemeMode::Dark), "dark"),
      (spelled(ThemeMode::Light), "light"),
      (spelled(NotificationPosition::TopLeft), "top_left"),
      (spelled(NotificationPosition::TopRight), "top_right"),
      (spelled(OsdPosition::TopCenter), "top_center"),
      (spelled(OsdPosition::BottomCenter), "bottom_center"),
      (spelled(OsdPosition::CenterLeft), "center_left"),
      (spelled(OsdPosition::CenterRight), "center_right"),
      (spelled(Weekday::Monday), "monday"),
      (spelled(Weekday::Sunday), "sunday"),
      (spelled(Units::Metric), "metric"),
      (spelled(Units::Imperial), "imperial"),
      (spelled(IdleAction::Lock), "lock"),
      (spelled(IdleAction::ScreenOff), "screen_off"),
      (spelled(IdleAction::Suspend), "suspend"),
      (spelled(IdleAction::LockAndSuspend), "lock_and_suspend"),
      (
        spelled(IdleAction::LockAndSuspendThenHibernate),
        "lock_and_suspend_then_hibernate",
      ),
      (spelled(IdleAction::Command), "command"),
    ];
    for (have, want) in cases {
      assert_eq!(have, want);
    }
    assert_eq!(round_trip(IdleAction::ScreenOff), IdleAction::ScreenOff);
    assert!(serde_json::from_str::<Placement>(r#""Top""#).is_err());
    assert!(serde_json::from_str::<OsdPosition>(r#""topCenter""#).is_err());
  }

  #[test]
  fn enum_defaults() {
    assert_eq!(
      NotificationPosition::default(),
      NotificationPosition::TopRight
    );
    assert_eq!(OsdPosition::default(), OsdPosition::BottomCenter);
    assert_eq!(Weekday::default(), Weekday::Monday);
    assert_eq!(Units::default(), Units::Metric);
    assert_eq!(IdleAction::default(), IdleAction::Command);
    assert_eq!(ThemeConfig::default().mode, None);
  }

  #[test]
  fn sections_default_per_field() {
    let config: Config =
      toml::from_str("[osd]\noffset = 3.0\n[osd.kinds]\nwifi = false\n").unwrap();
    assert_eq!(config.osd.offset, 3.);
    assert!(!config.osd.kinds.wifi);
    assert!(config.osd.kinds.volume);
    assert_eq!(config.osd.hide_delay_ms, OsdConfig::default().hide_delay_ms);
    // a file that names any bar replaces the default ones
    let config: Config = toml::from_str("[bar.side]\nposition = \"left\"\n").unwrap();
    assert_eq!(config.bar.keys().collect::<Vec<_>>(), ["side"]);
  }

  #[gpui::test]
  fn observe_section_fires_on_change_only(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Config::default()));
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let s = seen.clone();
    cx.update(|cx| {
      observe_section(
        cx,
        |c| &c.theme.name,
        move |name, _| s.borrow_mut().push(name.clone()),
      )
    });
    let set = |cx: &mut TestAppContext, edit: fn(&mut Config)| {
      cx.update(|cx| {
        let mut config = cx.config().clone();
        edit(&mut config);
        cx.set_global(config);
      });
    };
    set(cx, |c| c.osd.offset = 1.);
    assert!(seen.borrow().is_empty());
    set(cx, |c| c.theme.name = "X".into());
    set(cx, |c| c.theme.name = "X".into());
    set(cx, |c| c.theme.shadow = false);
    set(cx, |c| c.theme.name = "Y".into());
    assert_eq!(*seen.borrow(), ["X", "Y"]);
  }

  #[gpui::test]
  fn load_falls_back_to_defaults(cx: &mut TestAppContext) {
    let config = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_CONFIG_HOME", config.path());
      std::env::set_var("XDG_STATE_HOME", state.path());
    }
    let dir = config.path().join("corona");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.toml"), "[osd]\noffset = 7.0\n").unwrap();
    cx.update(load).unwrap();
    assert_eq!(cx.update(|cx| cx.config().osd.offset), 7.);

    std::fs::write(dir.join("b.toml"), "[osd\n").unwrap();
    assert!(cx.update(load).is_err());
    assert_eq!(cx.update(|cx| cx.config().clone()), Config::default());
  }
}
