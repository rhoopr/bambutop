//! Printer state model and MQTT message deserialization.
//!
//! Defines the core [`PrinterState`] struct and related types for tracking
//! printer status, temperatures, print progress, AMS state, and HMS errors.
//! State is incrementally updated from partial MQTT JSON messages via
//! [`PrinterState::update_from_message`].

use serde::Deserialize;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::time::Instant;

/// Special tray value indicating external spool (not in AMS).
/// Values >= this indicate no AMS tray is active (254=external, 255=none).
const TRAY_EXTERNAL_SPOOL: u8 = 254;

/// Bit shift to extract the module ID from an HMS attr field.
const HMS_MODULE_SHIFT: u32 = 24;
/// Bit shift to extract the severity from an HMS attr field.
const HMS_SEVERITY_SHIFT: u32 = 16;
/// Byte mask for extracting 8-bit HMS fields.
const HMS_BYTE_MASK: u32 = 0xFF;

/// Number of tray slots per AMS unit.
const AMS_TRAYS_PER_UNIT: u8 = 4;

/// Maximum number of AMS units supported (0-3, i.e. up to 4 units).
const MAX_AMS_UNITS: u8 = 4;

/// Number of tray bit positions per AMS unit in bitmask fields.
const TRAY_BITS_PER_UNIT: u8 = 4;

/// Bit offset for AMS HT (Hub Tray) in bitmask fields.
/// The AMS HT uses unit_id 128, mapped to bit position 16.
const AMS_HT_TRAY_BIT_OFFSET: u32 = 16;

/// Unit ID used by the AMS Hub Tray (AMS HT / external spool hub).
const AMS_HT_UNIT_ID: u8 = 128;

/// Maximum fan speed value in Bambu's 0-15 scale.
const BAMBU_FAN_SCALE_MAX: u32 = 15;
/// Percentage scale maximum.
const PERCENT_MAX: u32 = 100;

/// Speed level percentages for Bambu printers.
/// Levels: 1=silent, 2=standard, 3=sport, 4=ludicrous
const SPEED_SILENT: u32 = 50;
const SPEED_STANDARD: u32 = 100;
const SPEED_SPORT: u32 = 124;
const SPEED_LUDICROUS: u32 = 166;

/// Converts Bambu speed level (1-4) to its display name.
///
/// Returns "Standard" for unknown levels as a safe default.
pub fn speed_level_to_name(level: u8) -> &'static str {
    match level {
        1 => "Silent",
        2 => "Standard",
        3 => "Sport",
        4 => "Ludicrous",
        _ => "Standard",
    }
}

/// Converts Bambu speed level (1-4) to percentage.
///
/// Returns 100% (Standard) for unknown levels as a safe default.
pub fn speed_level_to_percent(level: u8) -> u32 {
    match level {
        1 => SPEED_SILENT,
        2 => SPEED_STANDARD,
        3 => SPEED_SPORT,
        4 => SPEED_LUDICROUS,
        _ => SPEED_STANDARD,
    }
}

/// Bitflags tracking which optional fields the printer has reported via MQTT.
///
/// Used for data-driven capability detection: instead of hardcoding model
/// names, we track what the printer actually sends. Fields that are never
/// received are hidden from the UI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReceivedFields(u16);

impl ReceivedFields {
    pub(crate) const HEATBREAK_FAN: u16 = 1 << 1;
    pub(crate) const XCAM: u16 = 1 << 2;
    pub(crate) const IPCAM: u16 = 1 << 3;
    pub(crate) const WORK_LIGHT: u16 = 1 << 4;
    pub(crate) const AUX_FAN: u16 = 1 << 5;
    pub(crate) const CHAMBER_FAN: u16 = 1 << 6;

    pub(crate) fn set(&mut self, flag: u16) {
        self.0 |= flag;
    }

    fn has(self, flag: u16) -> bool {
        self.0 & flag != 0
    }
}

/// Main printer state aggregated from MQTT messages.
///
/// This struct is incrementally updated from partial MQTT messages
/// sent by the printer. Each field may be updated independently.
#[derive(Debug, Clone, Default)]
pub struct PrinterState {
    /// Whether the MQTT connection to the printer is active
    pub connected: bool,
    /// User-configured printer name
    pub printer_name: String,
    /// Printer model derived from serial number prefix
    pub printer_model: String,
    /// Last 4 digits of serial number for compact display
    pub serial_suffix: String,
    /// Current print job status
    pub print_status: PrintStatus,
    /// Temperature readings for nozzle, bed, and chamber
    pub temperatures: Temperatures,
    /// Speed and fan settings
    pub speeds: Speeds,
    /// AMS (Automatic Material System) state, if present
    pub ams: Option<AmsState>,
    /// Chamber and work light states
    pub lights: LightState,
    /// WiFi signal strength (e.g., "-45dBm")
    pub wifi_signal: String,
    /// Active HMS (Health Management System) errors
    /// Uses SmallVec since there are typically 0-3 errors at a time
    pub hms_errors: SmallVec<[HmsError; 4]>,
    /// Whether HMS data has been received from the printer.
    /// Used to distinguish "no data yet" from "no errors".
    pub hms_received: bool,
    /// Firmware version string (e.g., "01.08.02.00")
    pub firmware_version: String,
    /// Hardware version string
    pub hardware_version: String,
    /// Nozzle diameter in mm (e.g., "0.4")
    pub nozzle_diameter: String,
    /// Heatbreak fan speed percentage (0-100)
    pub heatbreak_fan_speed: u8,
    /// Unix timestamp when current gcode started
    pub gcode_start_time: Option<u64>,
    /// Xcam monitoring state
    pub xcam: XcamState,
    /// IP camera state
    pub ipcam: IpcamState,
    /// Tracks which optional fields the printer has reported.
    /// Used for data-driven capability detection in the UI.
    pub received: ReceivedFields,
}

/// Temperature threshold (in degrees C) below target that indicates heating is in progress.
/// If current temp is more than this amount below target, we consider it "heating".
const HEATING_THRESHOLD: f32 = 5.0;

#[derive(Debug, Clone, Default)]
pub struct PrintStatus {
    pub gcode_file: String,
    pub subtask_name: String,
    pub project_id: String,
    pub task_id: String,
    pub progress: u8,
    pub layer_num: u32,
    pub total_layers: u32,
    pub remaining_time_mins: u32,
    pub gcode_state: String,
    pub print_type: String,
    /// Current print stage code from printer (stg_cur field)
    pub stage_code: i32,
}

/// Printer stage codes from `stg_cur` MQTT field.
///
/// Values sourced from the Home Assistant Bambu Lab integration
/// (`ha-bambulab` CURRENT_STAGE_IDS).
mod stage {
    /// Auto bed leveling
    pub const AUTO_LEVELING: i32 = 1;
    /// Heatbed preheating
    pub const HEATBED_PREHEATING: i32 = 2;
    /// Sweeping XY mech mode
    pub const SWEEPING_XY: i32 = 3;
    /// Changing filament / AMS operation
    pub const CHANGING_FILAMENT: i32 = 4;
    /// M400 pause (user-commanded pause)
    pub const M400_PAUSE: i32 = 5;
    /// Paused due to filament runout
    pub const FILAMENT_RUNOUT: i32 = 6;
    /// Heating hotend
    pub const HEATING_HOTEND: i32 = 7;
    /// Calibrating extrusion
    pub const CALIBRATING_EXTRUSION: i32 = 8;
    /// Scanning bed surface
    pub const SCANNING_BED: i32 = 9;
    /// Inspecting first layer
    pub const INSPECTING_FIRST_LAYER: i32 = 10;
    /// Identifying build plate type
    pub const IDENTIFYING_BUILD_PLATE: i32 = 11;
    /// Calibrating micro lidar
    pub const CALIBRATING_LIDAR: i32 = 12;
    /// Homing toolhead
    pub const HOMING: i32 = 13;
    /// Cleaning nozzle tip
    pub const CLEANING_NOZZLE: i32 = 14;
    /// Checking extruder temperature
    pub const CHECKING_EXTRUDER_TEMP: i32 = 15;
    /// Paused by user
    pub const USER_PAUSED: i32 = 16;
    /// Paused due to front cover falling
    pub const COVER_OPEN: i32 = 17;
    /// Calibrating micro lidar (secondary)
    pub const CALIBRATING_LIDAR_2: i32 = 18;
    /// Calibrating extrusion flow
    pub const CALIBRATING_FLOW: i32 = 19;
    /// Paused due to nozzle temperature malfunction
    pub const NOZZLE_TEMP_MALFUNCTION: i32 = 20;
    /// Paused due to heat bed temperature malfunction
    pub const BED_TEMP_MALFUNCTION: i32 = 21;
}

impl PrintStatus {
    /// Returns the best available display name for the print job.
    ///
    /// For cloud prints with only slicer profile info, shows "Cloud: <profile>".
    /// For local prints, shows the actual filename.
    ///
    /// Returns `Cow::Borrowed` when possible to avoid allocations.
    pub fn display_name(&self) -> Cow<'_, str> {
        let subtask = self.clean_name(&self.subtask_name);
        let gcode = self.clean_name(&self.gcode_file);

        // If subtask_name looks like an actual name (not slicer settings), use it
        if !subtask.is_empty() && !Self::looks_like_slicer_profile(&subtask) {
            return subtask;
        }

        // If gcode_file looks like an actual name, use it
        if !gcode.is_empty() && !Self::looks_like_slicer_profile(&gcode) {
            return gcode;
        }

        // We only have slicer profile info - format it nicely
        let profile = if !subtask.is_empty() { subtask } else { gcode };

        if profile.is_empty() {
            return Cow::Borrowed("");
        }

        // For cloud prints, prefix with "Cloud:" to make it clear
        if self.print_type == "cloud" {
            Cow::Owned(format!("Cloud: {}", profile))
        } else {
            profile
        }
    }

    /// Strips common file extensions from a name.
    /// Returns `Cow::Borrowed` when no trimming is needed.
    fn clean_name<'a>(&self, name: &'a str) -> Cow<'a, str> {
        let trimmed = name.trim();
        let cleaned = trimmed
            .trim_end_matches(".3mf")
            .trim_end_matches(".gcode")
            .trim_end_matches(".gcode.3mf");

        if cleaned.len() == trimmed.len() {
            Cow::Borrowed(trimmed)
        } else {
            Cow::Borrowed(cleaned)
        }
    }

    /// Checks if a name looks like slicer profile settings rather than a project name.
    fn looks_like_slicer_profile(name: &str) -> bool {
        let lower = name.to_lowercase();

        // Pattern: "0.2mm layer, 2 walls, 15% infill" style
        if lower.contains("mm layer") || lower.contains("% infill") || lower.contains(" walls") {
            return true;
        }

        // Pattern: contains multiple common profile terms
        let profile_terms = ["pla", "petg", "abs", "tpu", "draft", "quality", "strength"];
        let term_count = profile_terms.iter().filter(|t| lower.contains(*t)).count();
        term_count >= 2
    }

    /// Returns true if a print job is currently active (running or paused).
    pub fn is_active(&self) -> bool {
        matches!(self.gcode_state.as_str(), "RUNNING" | "PAUSE")
    }

    /// Determines the current print phase based on stage code and temperatures.
    ///
    /// Returns a human-readable phase description such as "Heating Bed", "Auto-Leveling",
    /// "Printing", etc. Uses the printer's stage code (stg_cur) when available,
    /// with fallback to temperature-based inference.
    ///
    /// # Arguments
    /// * `temps` - Current temperature readings to determine heating phases
    ///
    /// # Returns
    /// A static string describing the current phase, or `None` if no phase applies.
    pub fn print_phase(&self, temps: &Temperatures) -> Option<&'static str> {
        // Only show phase during active jobs
        if !self.is_active() {
            return None;
        }

        // Use stage code if available (more accurate than temperature inference)
        // Bambu stg_cur codes sourced from ha-bambulab CURRENT_STAGE_IDS.
        // See `mod stage` constants for the full mapping.
        // -1 = Idle (no stage), 0 = Printing (with progress > 0)
        match self.stage_code {
            stage::AUTO_LEVELING => return Some("Auto-Leveling"),
            stage::HEATBED_PREHEATING => return Some("Heating Bed"),
            stage::SWEEPING_XY => return Some("Sweeping XY"),
            stage::CHANGING_FILAMENT => return Some("Changing Filament"),
            stage::M400_PAUSE | stage::USER_PAUSED => return Some("Paused"),
            stage::FILAMENT_RUNOUT => return Some("Filament Runout"),
            stage::HEATING_HOTEND => return Some("Heating Nozzle"),
            stage::CALIBRATING_EXTRUSION | stage::CALIBRATING_FLOW => {
                return Some("Calibrating Extrusion");
            }
            stage::SCANNING_BED => return Some("Scanning Bed"),
            stage::INSPECTING_FIRST_LAYER => return Some("Inspecting First Layer"),
            stage::IDENTIFYING_BUILD_PLATE => return Some("Identifying Build Plate"),
            stage::CALIBRATING_LIDAR | stage::CALIBRATING_LIDAR_2 => {
                return Some("Calibrating Lidar");
            }
            stage::HOMING => return Some("Homing"),
            stage::CLEANING_NOZZLE => return Some("Cleaning Nozzle"),
            stage::CHECKING_EXTRUDER_TEMP => return Some("Checking Temperature"),
            stage::COVER_OPEN => return Some("Cover Open"),
            stage::NOZZLE_TEMP_MALFUNCTION | stage::BED_TEMP_MALFUNCTION => {
                return Some("Temperature Error");
            }
            _ => {}
        }

        // Fallback: infer from temperatures when stage code doesn't indicate a specific phase
        // This handles cases where stg_cur is 0 (printing/idle) or -1 (no stage)

        // Check if bed is still heating (target set but not reached)
        if temps.bed_target > 0.0 && temps.bed < temps.bed_target - HEATING_THRESHOLD {
            return Some("Heating Bed");
        }

        // Check if nozzle is still heating (target set but not reached)
        if temps.nozzle_target > 0.0 && temps.nozzle < temps.nozzle_target - HEATING_THRESHOLD {
            return Some("Heating Nozzle");
        }

        // If we have progress and layer info, we're actively printing
        if self.progress > 0 || self.layer_num > 0 {
            return Some("Printing");
        }

        // Default: preparing (active job but haven't started printing yet)
        Some("Preparing")
    }
}

#[derive(Debug, Clone, Default)]
pub struct Temperatures {
    pub nozzle: f32,
    pub nozzle_target: f32,
    pub bed: f32,
    pub bed_target: f32,
    pub chamber: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Speeds {
    pub speed_level: u8,
    pub fan_speed: u8,
    pub aux_fan_speed: u8,
    pub chamber_fan_speed: u8,
}

#[derive(Debug, Clone, Default)]
pub struct AmsState {
    /// AMS units (typically 1-4 units)
    /// Uses SmallVec since most setups have 1-4 AMS units
    pub units: SmallVec<[AmsUnit; 4]>,
    /// The currently active tray slot (0-3 within a unit)
    pub current_tray: Option<u8>,
    /// The currently active AMS unit index (0-3)
    pub current_unit: Option<u8>,
    /// Cached bitmask: which AMS units are physically present
    pub(crate) ams_exist_bits: u32,
    /// Cached bitmask: which tray slots have a tray inserted
    pub(crate) tray_exist_bits: u32,
    /// Cached bitmask: which trays contain Bambu-branded (BBL) filament
    pub(crate) tray_is_bbl_bits: u32,
    /// Cached bitmask: which trays have completed RFID tag reading
    pub(crate) tray_read_done_bits: u32,
    /// Cached bitmask: which trays are currently reading their RFID tag
    pub(crate) tray_reading_bits: u32,
}

#[derive(Debug, Clone, Default)]
pub struct AmsUnit {
    pub id: u8,
    pub humidity: u8,
    /// Tray slots in this AMS unit (typically 4, or 2 for AMS Lite)
    /// Uses SmallVec since AMS has exactly 4 slots (or 2 for Lite)
    pub trays: SmallVec<[AmsTray; 4]>,
    /// True if this is an AMS Lite unit (2 trays instead of 4)
    pub is_lite: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AmsTray {
    pub id: u8,
    pub material: String,
    pub remaining: u8,
    /// Pre-parsed RGB color values (r, g, b) for efficient rendering
    pub parsed_color: Option<(u8, u8, u8)>,
    /// Filament sub-brand (e.g., "Bambu PLA Basic")
    pub sub_brand: String,
    /// Recommended minimum nozzle temperature
    pub nozzle_temp_min: Option<i32>,
    /// Recommended maximum nozzle temperature
    pub nozzle_temp_max: Option<i32>,
    /// Whether this tray slot has a physical tray inserted (from tray_exist_bits)
    pub tray_exists: bool,
    /// Whether this tray contains Bambu-branded (BBL) filament (from tray_is_bbl_bits)
    pub is_bbl: bool,
    /// Whether RFID reading is complete for this tray (from tray_read_done_bits)
    pub read_done: bool,
    /// Whether RFID is currently being read for this tray (from tray_reading_bits)
    pub reading: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LightState {
    pub chamber_light: bool,
    pub work_light: bool,
}

/// HMS (Health Management System) error from the printer.
///
/// Some fields (`code`, `module`) are not currently used in the UI but are retained for:
/// - Debugging via the derived `Debug` impl
/// - Future features (e.g., linking to Bambu error documentation by code)
/// - Complete representation of printer error data
#[allow(dead_code)] // `module` field retained for Debug output and tests
#[derive(Debug, Clone)]
pub struct HmsError {
    pub code: u32,
    pub module: u8,
    pub severity: u8,
    pub message: String,
    /// When this error was first received from the printer
    pub received_at: Instant,
}

/// Xcam (AI monitoring) state from the printer.
#[derive(Debug, Clone, Default)]
pub struct XcamState {
    /// Whether spaghetti detection is enabled
    pub spaghetti_detector: bool,
    /// Whether first layer inspection is enabled
    pub first_layer_inspector: bool,
    /// Whether to halt print on detection
    pub print_halt: bool,
}

/// IP camera and timelapse state.
#[derive(Debug, Clone, Default)]
pub struct IpcamState {
    /// Whether the camera is recording
    pub recording: bool,
    /// Whether timelapse is enabled
    pub timelapse: bool,
    /// Camera resolution (e.g., "1080p")
    pub resolution: String,
}

/// Raw MQTT message structure from Bambu printer
#[derive(Debug, Deserialize)]
pub struct MqttMessage {
    pub print: Option<PrintReport>,
    pub info: Option<InfoReport>,
}

/// Info report from MQTT containing version and module information.
///
/// Sent in response to a `get_version` info request.
#[derive(Debug, Deserialize)]
pub struct InfoReport {
    pub module: Option<Vec<InfoModule>>,
}

/// A single module entry from the info report.
#[derive(Debug, Deserialize)]
pub struct InfoModule {
    pub name: Option<String>,
    pub sw_ver: Option<String>,
    pub hw_ver: Option<String>,
}

/// Raw print report from MQTT containing all fields sent by the printer.
///
/// Many fields are not currently used by the application but are retained for:
/// - Documentation of the full Bambu MQTT protocol
/// - Future feature development without re-discovering field names
/// - Serde deserialization (unknown fields would otherwise cause errors)
#[allow(dead_code)] // Fields deserialized from MQTT JSON but not all read from Rust
#[derive(Debug, Default, Deserialize)]
pub struct PrintReport {
    // Print job info
    pub gcode_file: Option<String>,
    pub subtask_name: Option<String>,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub profile_id: Option<String>,
    pub subtask_id: Option<String>,
    #[serde(rename = "mc_percent")]
    pub progress: Option<u8>,
    pub layer_num: Option<u32>,
    pub total_layer_num: Option<u32>,
    #[serde(rename = "mc_remaining_time")]
    pub remaining_time: Option<u32>,
    pub gcode_state: Option<String>,
    pub print_type: Option<String>,
    /// Current stage code (stg_cur): indicates what the printer is doing
    /// Common values: 0=idle, 1=auto-leveling, 2=heatbed preheating,
    /// 6=cleaning nozzle, 7=calibrating extrusion, 14=printing, etc.
    pub stg_cur: Option<i32>,

    // Temperatures
    pub nozzle_temper: Option<f32>,
    pub nozzle_target_temper: Option<f32>,
    pub bed_temper: Option<f32>,
    pub bed_target_temper: Option<f32>,
    pub chamber_temper: Option<f32>,

    // Speeds & fans
    pub spd_lvl: Option<u8>,
    pub cooling_fan_speed: Option<String>,
    pub big_fan1_speed: Option<String>,
    pub big_fan2_speed: Option<String>,

    // Lights
    pub lights_report: Option<Vec<LightReport>>,

    // AMS
    pub ams: Option<AmsReport>,
    pub ams_status: Option<u32>,

    // Misc
    pub wifi_signal: Option<String>,

    // Printer info
    pub machine_name: Option<String>,
    pub hw_ver: Option<String>,
    pub sw_ver: Option<String>,
    pub nozzle_diameter: Option<String>,
    pub heatbreak_fan_speed: Option<serde_json::Value>,
    pub gcode_start_time: Option<serde_json::Value>,
    pub xcam: Option<XcamReport>,
    pub ipcam: Option<IpcamReport>,

    // HMS errors
    pub hms: Option<Vec<HmsReport>>,
}

#[derive(Debug, Deserialize)]
pub struct LightReport {
    pub node: String,
    pub mode: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct AmsReport {
    pub ams: Option<Vec<AmsUnitReport>>,
    pub tray_now: Option<String>,
    /// Hex bitmask: which AMS units are physically present
    pub ams_exist_bits: Option<String>,
    /// Hex bitmask: which tray slots have a tray inserted
    pub tray_exist_bits: Option<String>,
    /// Hex bitmask: which trays contain Bambu-branded (BBL) filament
    pub tray_is_bbl_bits: Option<String>,
    /// Hex bitmask: which trays have completed RFID tag reading
    pub tray_read_done_bits: Option<String>,
    /// Hex bitmask: which trays are currently reading their RFID tag
    pub tray_reading_bits: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AmsUnitReport {
    pub id: String,
    pub humidity: String,
    pub tray: Option<Vec<AmsTrayReport>>,
}

#[derive(Debug, Default, Deserialize)]
pub struct AmsTrayReport {
    pub id: String,
    pub tray_type: Option<String>,
    pub tray_color: Option<String>,
    pub remain: Option<i32>,
    pub tray_sub_brands: Option<String>,
    pub nozzle_temp_min: Option<String>,
    pub nozzle_temp_max: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HmsReport {
    pub attr: u32,
    pub code: u32,
}

/// Xcam report from MQTT. Bambu printers send xcam fields inconsistently
/// (booleans as strings, ints, or actual bools), so we accept any JSON value
/// and parse manually. Unknown fields are ignored.
#[derive(Debug, Deserialize)]
pub struct XcamReport {
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub first_layer_inspector: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub print_halt: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_bool_flexible")]
    pub spaghetti_detector: Option<bool>,
}

/// IP camera report from MQTT. Unknown fields are ignored.
#[derive(Debug, Deserialize)]
pub struct IpcamReport {
    pub ipcam_record: Option<String>,
    pub timelapse: Option<String>,
    pub resolution: Option<String>,
}

/// Deserializes a bool that may arrive as bool, integer, or string from MQTT.
fn deserialize_bool_flexible<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct BoolVisitor;

    impl<'de> de::Visitor<'de> for BoolVisitor {
        type Value = Option<bool>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a bool, integer, or string")
        }

        fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
            Ok(Some(v))
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Some(v != 0))
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Some(v != 0))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            match v {
                "true" | "1" | "enable" => Ok(Some(true)),
                "false" | "0" | "disable" => Ok(Some(false)),
                _ => Ok(None),
            }
        }

        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
    }

    deserializer.deserialize_any(BoolVisitor)
}

impl PrinterState {
    pub fn update_from_message(&mut self, msg: &MqttMessage) {
        if let Some(print) = &msg.print {
            self.update_from_print_report(print);
        }
        if let Some(info) = &msg.info {
            self.update_from_info_report(info);
        }
    }

    /// Extracts firmware and hardware versions from the info report.
    ///
    /// The "ota" module contains the main firmware version.
    fn update_from_info_report(&mut self, report: &InfoReport) {
        if let Some(modules) = &report.module {
            for module in modules {
                let is_ota = module.name.as_deref().is_some_and(|n| n == "ota");
                if is_ota {
                    if let Some(v) = &module.sw_ver {
                        self.firmware_version.clone_from(v);
                    }
                    if let Some(v) = &module.hw_ver {
                        self.hardware_version.clone_from(v);
                    }
                    break;
                }
            }
        }
    }

    fn update_from_print_report(&mut self, report: &PrintReport) {
        // Print status - use clone_from to potentially reuse allocations
        if let Some(v) = &report.gcode_file {
            self.print_status.gcode_file.clone_from(v);
        }
        if let Some(v) = &report.subtask_name {
            self.print_status.subtask_name.clone_from(v);
        }
        if let Some(v) = &report.project_id {
            self.print_status.project_id.clone_from(v);
        }
        if let Some(v) = &report.task_id {
            self.print_status.task_id.clone_from(v);
        }
        if let Some(v) = report.progress {
            self.print_status.progress = v;
        }
        if let Some(v) = report.layer_num {
            self.print_status.layer_num = v;
        }
        if let Some(v) = report.total_layer_num {
            self.print_status.total_layers = v;
        }
        if let Some(v) = report.remaining_time {
            self.print_status.remaining_time_mins = v;
        }
        if let Some(v) = &report.gcode_state {
            self.print_status.gcode_state.clone_from(v);
        }
        if let Some(v) = &report.print_type {
            self.print_status.print_type.clone_from(v);
        }
        if let Some(v) = report.stg_cur {
            self.print_status.stage_code = v;
        }

        // Temperatures
        if let Some(v) = report.nozzle_temper {
            self.temperatures.nozzle = v;
        }
        if let Some(v) = report.nozzle_target_temper {
            self.temperatures.nozzle_target = v;
        }
        if let Some(v) = report.bed_temper {
            self.temperatures.bed = v;
        }
        if let Some(v) = report.bed_target_temper {
            self.temperatures.bed_target = v;
        }
        if let Some(v) = report.chamber_temper {
            self.temperatures.chamber = v;
        }

        // Speeds
        if let Some(v) = report.spd_lvl {
            self.speeds.speed_level = v;
        }
        if let Some(v) = &report.cooling_fan_speed {
            if let Some(speed) = parse_fan_speed(v) {
                self.speeds.fan_speed = speed;
            }
        }
        if let Some(v) = &report.big_fan1_speed {
            if let Some(speed) = parse_fan_speed(v) {
                self.speeds.aux_fan_speed = speed;
                self.received.set(ReceivedFields::AUX_FAN);
            }
        }
        if let Some(v) = &report.big_fan2_speed {
            if let Some(speed) = parse_fan_speed(v) {
                self.speeds.chamber_fan_speed = speed;
                self.received.set(ReceivedFields::CHAMBER_FAN);
            }
        }

        // Lights
        if let Some(lights) = &report.lights_report {
            for light in lights {
                match light.node.as_str() {
                    "chamber_light" => self.lights.chamber_light = light.mode == "on",
                    "work_light" => {
                        self.lights.work_light = light.mode == "on";
                        self.received.set(ReceivedFields::WORK_LIGHT);
                    }
                    _ => {}
                }
            }
        }

        // WiFi signal - store raw string value (e.g., "-45dBm")
        if let Some(v) = &report.wifi_signal {
            self.wifi_signal.clone_from(v);
        }

        // AMS
        if let Some(ams_report) = &report.ams {
            self.update_ams(ams_report);
        }

        // HMS errors
        if let Some(hms_list) = &report.hms {
            self.hms_received = true;
            let now = Instant::now();
            self.hms_errors = hms_list
                .iter()
                .map(|h| HmsError {
                    code: h.code,
                    module: ((h.attr >> HMS_MODULE_SHIFT) & HMS_BYTE_MASK) as u8,
                    severity: ((h.attr >> HMS_SEVERITY_SHIFT) & HMS_BYTE_MASK) as u8,
                    message: format_hms_code(h.code).into_owned(),
                    received_at: now,
                })
                .collect();
        }

        // Printer info
        if let Some(v) = &report.machine_name {
            self.printer_name.clone_from(v);
        }

        // Firmware/hardware versions
        if let Some(v) = &report.hw_ver {
            self.hardware_version.clone_from(v);
        }
        if let Some(v) = &report.sw_ver {
            self.firmware_version.clone_from(v);
        }

        // Nozzle diameter
        if let Some(v) = &report.nozzle_diameter {
            self.nozzle_diameter.clone_from(v);
        }

        // Heatbreak fan speed (can be string or number)
        if let Some(v) = &report.heatbreak_fan_speed {
            let speed_str = v
                .as_str()
                .map(String::from)
                .or_else(|| v.as_u64().map(|n| n.to_string()));
            if let Some(ref s) = speed_str {
                if let Some(speed) = parse_fan_speed(s) {
                    self.heatbreak_fan_speed = speed;
                    self.received.set(ReceivedFields::HEATBREAK_FAN);
                }
            }
        }

        // Gcode start time (can be number or string)
        if let Some(v) = &report.gcode_start_time {
            let ts = v
                .as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()));
            if let Some(ts) = ts {
                if ts > 0 {
                    self.gcode_start_time = Some(ts);
                }
            }
        }

        // Xcam monitoring
        if let Some(xcam) = &report.xcam {
            self.received.set(ReceivedFields::XCAM);
            if let Some(v) = xcam.spaghetti_detector {
                self.xcam.spaghetti_detector = v;
            }
            if let Some(v) = xcam.first_layer_inspector {
                self.xcam.first_layer_inspector = v;
            }
            if let Some(v) = xcam.print_halt {
                self.xcam.print_halt = v;
            }
        }

        // IP camera
        if let Some(ipcam) = &report.ipcam {
            self.received.set(ReceivedFields::IPCAM);
            if let Some(v) = &ipcam.ipcam_record {
                self.ipcam.recording = v == "enable";
            }
            if let Some(v) = &ipcam.timelapse {
                self.ipcam.timelapse = v == "enable";
            }
            if let Some(v) = &ipcam.resolution {
                self.ipcam.resolution.clone_from(v);
            }
        }
    }

    /// Set model and serial suffix from serial number.
    ///
    /// Derives the printer model from the serial prefix and extracts
    /// the last 4 digits for compact display in the UI header.
    pub fn set_model_from_serial(&mut self, serial: &str) {
        self.printer_model = model_from_serial(serial).to_string();
        // Store last 4 characters of serial for compact title display
        if serial.len() >= 4 {
            self.serial_suffix = serial[serial.len() - 4..].to_string();
        }
    }

    /// Returns the material type of the currently active AMS tray.
    ///
    /// Returns `None` if:
    /// - No AMS is present
    /// - No tray is currently selected (external spool or idle)
    /// - The active tray has no material loaded
    pub fn active_filament_type(&self) -> Option<&str> {
        let ams = self.ams.as_ref()?;
        let unit_idx = ams.current_unit? as usize;
        let tray_idx = ams.current_tray? as usize;

        let unit = ams.units.get(unit_idx)?;
        let tray = unit.trays.get(tray_idx)?;

        if tray.material.is_empty() {
            None
        } else {
            Some(&tray.material)
        }
    }

    /// Returns true if the printer model has a chamber temperature sensor.
    ///
    /// Only enclosed printers (X1, P2S, H2 series) have real chamber sensors.
    /// Open-frame printers (A1 series) report ambient noise values via MQTT
    /// and should not display chamber temperature.
    pub fn has_chamber_temp_sensor(&self) -> bool {
        model_has_chamber(&self.printer_model)
    }

    /// Returns true if the printer has reported heatbreak fan speed data.
    pub fn has_heatbreak_fan(&self) -> bool {
        self.received.has(ReceivedFields::HEATBREAK_FAN)
    }

    /// Returns true if the printer has reported xcam (AI monitoring) data.
    pub fn has_xcam(&self) -> bool {
        self.received.has(ReceivedFields::XCAM)
    }

    /// Returns true if the printer has reported IP camera data.
    pub fn has_ipcam(&self) -> bool {
        self.received.has(ReceivedFields::IPCAM)
    }

    /// Returns true if the printer has reported a work light.
    pub fn has_work_light(&self) -> bool {
        self.received.has(ReceivedFields::WORK_LIGHT)
    }

    /// Returns true if the printer has reported aux fan speed data.
    pub fn has_aux_fan(&self) -> bool {
        self.received.has(ReceivedFields::AUX_FAN)
    }

    /// Returns true if the printer has reported chamber fan speed data.
    pub fn has_chamber_fan(&self) -> bool {
        self.received.has(ReceivedFields::CHAMBER_FAN)
    }

    fn update_ams(&mut self, report: &AmsReport) {
        let mut ams_state = self.ams.take().unwrap_or_default();

        // Parse tray_now to determine both active unit and tray slot
        // Format: tray_now is a combined value where:
        // - For AMS: value = (unit_id * 4) + tray_id (e.g., "5" = unit 1, tray 1)
        // - Special values: "254" = external spool, "255" = no tray selected
        if let Some(tray) = &report.tray_now {
            if let Ok(tray_val) = tray.parse::<u8>() {
                if tray_val < TRAY_EXTERNAL_SPOOL {
                    // Calculate unit and slot from combined tray value
                    let unit = tray_val / AMS_TRAYS_PER_UNIT;
                    if unit < MAX_AMS_UNITS {
                        ams_state.current_unit = Some(unit);
                        ams_state.current_tray = Some(tray_val % AMS_TRAYS_PER_UNIT);
                    } else {
                        // Out-of-range unit index, treat as no selection
                        ams_state.current_unit = None;
                        ams_state.current_tray = None;
                    }
                } else {
                    // External spool (254) or no selection (255)
                    ams_state.current_unit = None;
                    ams_state.current_tray = None;
                }
            }
        }

        // Parse bitmask fields, falling back to cached values when absent
        if let Some(v) = &report.ams_exist_bits {
            ams_state.ams_exist_bits = parse_hex_bitmask(v);
        }
        if let Some(v) = &report.tray_exist_bits {
            ams_state.tray_exist_bits = parse_hex_bitmask(v);
        }
        if let Some(v) = &report.tray_is_bbl_bits {
            ams_state.tray_is_bbl_bits = parse_hex_bitmask(v);
        }
        if let Some(v) = &report.tray_read_done_bits {
            ams_state.tray_read_done_bits = parse_hex_bitmask(v);
        }
        if let Some(v) = &report.tray_reading_bits {
            ams_state.tray_reading_bits = parse_hex_bitmask(v);
        }

        if let Some(units) = &report.ams {
            ams_state.units = units
                .iter()
                .map(|u| {
                    let unit_id: u8 = u.id.parse().unwrap_or(0);
                    let trays: SmallVec<[AmsTray; 4]> = u
                        .tray
                        .as_ref()
                        .map(|trays| {
                            trays
                                .iter()
                                .map(|t| {
                                    let tray_id: u8 = t.id.parse().unwrap_or(0);
                                    let color_str = t.tray_color.as_deref().unwrap_or_default();
                                    AmsTray {
                                        id: tray_id,
                                        material: t.tray_type.clone().unwrap_or_default(),
                                        remaining: t.remain.unwrap_or(0).max(0) as u8,
                                        parsed_color: parse_hex_color(color_str),
                                        sub_brand: t
                                            .tray_sub_brands
                                            .as_deref()
                                            .unwrap_or_default()
                                            .to_string(),
                                        nozzle_temp_min: t
                                            .nozzle_temp_min
                                            .as_deref()
                                            .and_then(|s| s.parse().ok()),
                                        nozzle_temp_max: t
                                            .nozzle_temp_max
                                            .as_deref()
                                            .and_then(|s| s.parse().ok()),
                                        tray_exists: tray_bit_set(
                                            ams_state.tray_exist_bits,
                                            unit_id,
                                            tray_id,
                                        ),
                                        is_bbl: tray_bit_set(
                                            ams_state.tray_is_bbl_bits,
                                            unit_id,
                                            tray_id,
                                        ),
                                        read_done: tray_bit_set(
                                            ams_state.tray_read_done_bits,
                                            unit_id,
                                            tray_id,
                                        ),
                                        reading: tray_bit_set(
                                            ams_state.tray_reading_bits,
                                            unit_id,
                                            tray_id,
                                        ),
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    // Detect AMS Lite: has only 2 tray slots instead of 4
                    // AMS Lite units report fewer trays or have humidity value of 0
                    let is_lite = trays.len() <= 2 && !trays.is_empty();

                    AmsUnit {
                        id: unit_id,
                        humidity: u.humidity.parse().unwrap_or(0),
                        trays,
                        is_lite,
                    }
                })
                .collect();
        }

        self.ams = Some(ams_state);
    }
}

/// Parses fan speed from Bambu's 0-15 scale string to percentage (0-100).
///
/// Returns `None` if the string cannot be parsed as a valid number.
/// Valid input: "0" to "15" representing the Bambu fan speed scale.
fn parse_fan_speed(s: &str) -> Option<u8> {
    let val: u32 = s.parse().ok()?;
    // Bambu uses 0-15 scale, convert to percentage
    // Cap at max to prevent overflow in edge cases
    // Round to match Bambu Handy display
    let capped = val.min(BAMBU_FAN_SCALE_MAX);
    Some(((capped as f32 / BAMBU_FAN_SCALE_MAX as f32) * PERCENT_MAX as f32).round() as u8)
}

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some((r, g, b))
}

/// Parses a hex string (e.g., "3C" or "0x3C") into a u32 bitmask.
///
/// Returns 0 on invalid input, which is safe since 0 means "no bits set".
fn parse_hex_bitmask(hex: &str) -> u32 {
    let hex = hex.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(hex, 16).unwrap_or(0)
}

/// Tests if a specific tray's bit is set in a bitmask.
///
/// Standard AMS units (0-3) use bit position `unit_id * 4 + tray_id`.
/// AMS HT (unit 128) is mapped to bit offset 16.
/// Returns false for out-of-range bit positions (>= 32).
fn tray_bit_set(bitmask: u32, unit_id: u8, tray_id: u8) -> bool {
    let bit_offset = if unit_id == AMS_HT_UNIT_ID {
        AMS_HT_TRAY_BIT_OFFSET + u32::from(tray_id)
    } else {
        u32::from(unit_id) * u32::from(TRAY_BITS_PER_UNIT) + u32::from(tray_id)
    };
    if bit_offset >= 32 {
        return false;
    }
    bitmask & (1 << bit_offset) != 0
}

fn format_hms_code(code: u32) -> Cow<'static, str> {
    // HMS error code lookup - common codes from Bambu documentation
    match code {
        // AMS errors (0x0700xxxx)
        0x0700_0001 => Cow::Borrowed("AMS: Filament runout"),
        0x0700_0002 => Cow::Borrowed("AMS: Filament broken"),
        0x0700_0003 => Cow::Borrowed("AMS: Filament tangled"),
        0x0700_0004 => Cow::Borrowed("AMS: Filament unloading failed"),
        0x0700_0005 => Cow::Borrowed("AMS: Filament loading failed"),
        0x0700_0006 => Cow::Borrowed("AMS: Slot empty"),
        0x0700_0100 => Cow::Borrowed("AMS: Assist motor overload"),
        0x0700_0200 => Cow::Borrowed("AMS: Cutter error"),
        0x0700_0300 => Cow::Borrowed("AMS: Filament may be tangled"),
        0x0700_0400 => Cow::Borrowed("AMS: RFID read error"),
        0x0700_0500 => Cow::Borrowed("AMS: AMS communication error"),
        0x0700_1000 => Cow::Borrowed("AMS: Humidity sensor error"),

        // Nozzle/hotend errors (0x0300xxxx)
        0x0300_0001 => Cow::Borrowed("Nozzle: Temperature too high"),
        0x0300_0002 => Cow::Borrowed("Nozzle: Temperature too low"),
        0x0300_0003 => Cow::Borrowed("Nozzle: Temperature abnormal"),
        0x0300_0100 => Cow::Borrowed("Nozzle: Heater error"),
        0x0300_0200 => Cow::Borrowed("Nozzle: Thermistor error"),
        0x0300_0300 => Cow::Borrowed("Nozzle: Clogged"),

        // Bed errors (0x0400xxxx)
        0x0400_0001 => Cow::Borrowed("Bed: Temperature too high"),
        0x0400_0002 => Cow::Borrowed("Bed: Temperature too low"),
        0x0400_0100 => Cow::Borrowed("Bed: Heater error"),
        0x0400_0200 => Cow::Borrowed("Bed: Thermistor error"),

        // Motion errors (0x0500xxxx)
        0x0500_0001 => Cow::Borrowed("Motion: X-axis homing failed"),
        0x0500_0002 => Cow::Borrowed("Motion: Y-axis homing failed"),
        0x0500_0003 => Cow::Borrowed("Motion: Z-axis homing failed"),
        0x0500_0100 => Cow::Borrowed("Motion: X-axis motor error"),
        0x0500_0200 => Cow::Borrowed("Motion: Y-axis motor error"),
        0x0500_0300 => Cow::Borrowed("Motion: Z-axis motor error"),
        0x0500_0400 => Cow::Borrowed("Motion: Extruder motor error"),

        // Print errors (0x0C00xxxx)
        0x0C00_0001 => Cow::Borrowed("Print: First layer inspection failed"),
        0x0C00_0002 => Cow::Borrowed("Print: Spaghetti detected"),
        0x0C00_0003 => Cow::Borrowed("Print: Foreign object on bed"),
        0x0C00_0100 => Cow::Borrowed("Print: Build plate not detected"),
        0x0C00_0200 => Cow::Borrowed("Print: Auto-leveling failed"),
        0x0C00_0300 => Cow::Borrowed("Print: Nozzle height abnormal"),

        // System errors (0x0800xxxx)
        0x0800_0001 => Cow::Borrowed("System: SD card error"),
        0x0800_0002 => Cow::Borrowed("System: Storage full"),
        0x0800_0100 => Cow::Borrowed("System: Camera error"),
        0x0800_0200 => Cow::Borrowed("System: WiFi disconnected"),
        0x0800_0300 => Cow::Borrowed("System: Chamber door open"),
        0x0800_0400 => Cow::Borrowed("System: Front cover removed"),

        // Fallback for unknown codes
        _ => Cow::Borrowed("See wiki.bambulab.com"),
    }
}

fn model_from_serial(serial: &str) -> &'static str {
    // Bambu serial number prefixes indicate model
    // Format: XXYYYZZ... where XX indicates model
    if serial.len() < 3 {
        return "Bambu Printer";
    }

    // Prefixes from: https://wiki.bambulab.com/en/general/find-sn
    // Note: 01P/01S are counterintuitively swapped (01P=P1S, 01S=P1P)
    match &serial[..3] {
        // P1 Series
        "01P" => "Bambu Lab P1S",
        "01S" => "Bambu Lab P1P",
        "22E" => "Bambu Lab P2S",
        // X1 Series
        "00M" => "Bambu Lab X1C",
        "03W" => "Bambu Lab X1E",
        // A1 Series
        "030" => "Bambu Lab A1 Mini",
        "039" => "Bambu Lab A1",
        // H2 Series
        "31B" => "Bambu Lab H2C",
        "093" => "Bambu Lab H2S",
        "094" => "Bambu Lab H2D",
        "239" => "Bambu Lab H2D Pro",
        _ => "Bambu Printer",
    }
}

/// Returns true if the printer model has a chamber temperature sensor.
///
/// Only certain models have a real sensor:
/// - X1 series: sensor on the button board
/// - P2S: NTC sensor in the front beam
/// - H2 series: active chamber heating with sensor
///
/// The P1S/P1P have a chamber regulator fan but no temperature sensor.
/// Open-frame printers (A1 series) report ambient noise via `chamber_temper`.
fn model_has_chamber(model: &str) -> bool {
    matches!(
        model,
        "Bambu Lab X1C"
            | "Bambu Lab X1E"
            | "Bambu Lab P2S"
            | "Bambu Lab H2C"
            | "Bambu Lab H2S"
            | "Bambu Lab H2D"
            | "Bambu Lab H2D Pro"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    mod speed_level_to_percent_tests {
        use super::*;

        #[test]
        fn converts_known_speed_levels() {
            assert_eq!(speed_level_to_percent(1), 50); // Silent
            assert_eq!(speed_level_to_percent(2), 100); // Standard
            assert_eq!(speed_level_to_percent(3), 124); // Sport
            assert_eq!(speed_level_to_percent(4), 166); // Ludicrous
        }

        #[test]
        fn defaults_unknown_levels_to_standard() {
            assert_eq!(speed_level_to_percent(0), 100);
            assert_eq!(speed_level_to_percent(5), 100);
            assert_eq!(speed_level_to_percent(255), 100);
        }
    }

    mod speed_level_to_name_tests {
        use super::*;

        #[test]
        fn converts_known_speed_levels() {
            assert_eq!(speed_level_to_name(1), "Silent");
            assert_eq!(speed_level_to_name(2), "Standard");
            assert_eq!(speed_level_to_name(3), "Sport");
            assert_eq!(speed_level_to_name(4), "Ludicrous");
        }

        #[test]
        fn defaults_unknown_levels_to_standard() {
            assert_eq!(speed_level_to_name(0), "Standard");
            assert_eq!(speed_level_to_name(5), "Standard");
            assert_eq!(speed_level_to_name(255), "Standard");
        }
    }

    mod parse_fan_speed_tests {
        use super::*;

        #[test]
        fn converts_zero() {
            assert_eq!(parse_fan_speed("0"), Some(0));
        }

        #[test]
        fn converts_max() {
            assert_eq!(parse_fan_speed("15"), Some(100));
        }

        #[test]
        fn converts_mid_values() {
            // 7/15 * 100 = 46.67, rounded to 47
            assert_eq!(parse_fan_speed("7"), Some(47));
            // 8/15 * 100 = 53.33, rounded to 53
            assert_eq!(parse_fan_speed("8"), Some(53));
        }

        #[test]
        fn caps_values_above_15() {
            // Values above 15 are capped to prevent overflow
            assert_eq!(parse_fan_speed("20"), Some(100));
            assert_eq!(parse_fan_speed("255"), Some(100));
        }

        #[test]
        fn returns_none_for_invalid_input() {
            assert_eq!(parse_fan_speed("invalid"), None);
            assert_eq!(parse_fan_speed(""), None);
            assert_eq!(parse_fan_speed("-1"), None);
        }

        #[test]
        fn returns_none_for_whitespace() {
            // Whitespace around numbers is not handled by parse()
            assert_eq!(parse_fan_speed(" 10 "), None);
            assert_eq!(parse_fan_speed(" 10"), None);
            assert_eq!(parse_fan_speed("10 "), None);
        }
    }

    mod parse_hex_color_tests {
        use super::*;

        #[test]
        fn parses_with_hash_prefix() {
            assert_eq!(parse_hex_color("#FF0000"), Some((255, 0, 0)));
            assert_eq!(parse_hex_color("#00FF00"), Some((0, 255, 0)));
            assert_eq!(parse_hex_color("#0000FF"), Some((0, 0, 255)));
        }

        #[test]
        fn parses_without_hash_prefix() {
            assert_eq!(parse_hex_color("FF0000"), Some((255, 0, 0)));
            assert_eq!(parse_hex_color("00FF00"), Some((0, 255, 0)));
        }

        #[test]
        fn parses_lowercase() {
            assert_eq!(parse_hex_color("ff00ff"), Some((255, 0, 255)));
            assert_eq!(parse_hex_color("#aabbcc"), Some((170, 187, 204)));
        }

        #[test]
        fn returns_none_for_short_strings() {
            assert_eq!(parse_hex_color("FF00"), None);
            assert_eq!(parse_hex_color("#FFF"), None);
            assert_eq!(parse_hex_color(""), None);
        }

        #[test]
        fn returns_none_for_invalid_hex() {
            assert_eq!(parse_hex_color("GGGGGG"), None);
            assert_eq!(parse_hex_color("not-hex"), None);
        }

        #[test]
        fn ignores_alpha_channel() {
            // Only first 6 hex chars are used, alpha is ignored
            assert_eq!(parse_hex_color("FF0000FF"), Some((255, 0, 0)));
        }
    }

    mod format_hms_code_tests {
        use super::*;

        #[test]
        fn returns_borrowed_for_known_codes() {
            let result = format_hms_code(0x0700_0001);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "AMS: Filament runout");
        }

        #[test]
        fn returns_borrowed_for_unknown_codes() {
            let result = format_hms_code(0x9999_9999);
            assert!(matches!(result, Cow::Borrowed(_)));
            assert_eq!(result, "See wiki.bambulab.com");
        }

        #[test]
        fn maps_common_error_codes() {
            assert_eq!(format_hms_code(0x0300_0300), "Nozzle: Clogged");
            assert_eq!(format_hms_code(0x0400_0001), "Bed: Temperature too high");
            assert_eq!(format_hms_code(0x0C00_0002), "Print: Spaghetti detected");
        }
    }

    mod model_from_serial_tests {
        use super::*;

        #[test]
        fn identifies_p1_series() {
            assert_eq!(model_from_serial("01P00A000000000"), "Bambu Lab P1S");
            assert_eq!(model_from_serial("01S00A000000000"), "Bambu Lab P1P");
        }

        #[test]
        fn identifies_x1_series() {
            assert_eq!(model_from_serial("00M00A000000000"), "Bambu Lab X1C");
            assert_eq!(model_from_serial("03W00A000000000"), "Bambu Lab X1E");
        }

        #[test]
        fn identifies_a1_series() {
            assert_eq!(model_from_serial("03000A000000000"), "Bambu Lab A1 Mini");
            assert_eq!(model_from_serial("03900A000000000"), "Bambu Lab A1");
        }

        #[test]
        fn returns_default_for_unknown() {
            assert_eq!(model_from_serial("XXX00000000000"), "Bambu Printer");
        }

        #[test]
        fn returns_default_for_short_serial() {
            assert_eq!(model_from_serial("01"), "Bambu Printer");
            assert_eq!(model_from_serial(""), "Bambu Printer");
        }
    }

    mod display_name_tests {
        use super::*;

        #[test]
        fn prefers_subtask_name_over_gcode_file() {
            let status = PrintStatus {
                subtask_name: "My Project".to_string(),
                gcode_file: "some_file.gcode".to_string(),
                ..Default::default()
            };
            assert_eq!(status.display_name(), "My Project");
        }

        #[test]
        fn falls_back_to_gcode_file() {
            let status = PrintStatus {
                subtask_name: "".to_string(),
                gcode_file: "My Model.gcode".to_string(),
                ..Default::default()
            };
            assert_eq!(status.display_name(), "My Model");
        }

        #[test]
        fn strips_file_extensions() {
            let status = PrintStatus {
                subtask_name: "Model.3mf".to_string(),
                ..Default::default()
            };
            assert_eq!(status.display_name(), "Model");

            let status2 = PrintStatus {
                subtask_name: "Model.gcode".to_string(),
                ..Default::default()
            };
            assert_eq!(status2.display_name(), "Model");

            // Chained extension: .gcode.3mf
            let status3 = PrintStatus {
                subtask_name: "Model.gcode.3mf".to_string(),
                ..Default::default()
            };
            assert_eq!(status3.display_name(), "Model");
        }

        #[test]
        fn detects_slicer_profiles_with_cloud_prefix() {
            let status = PrintStatus {
                subtask_name: "0.2mm layer, 2 walls, 15% infill".to_string(),
                gcode_file: "".to_string(),
                print_type: "cloud".to_string(),
                ..Default::default()
            };
            assert_eq!(
                status.display_name(),
                "Cloud: 0.2mm layer, 2 walls, 15% infill"
            );
        }

        #[test]
        fn slicer_profile_without_cloud_shows_raw() {
            // Non-cloud print with only slicer profile info
            let status = PrintStatus {
                subtask_name: "0.2mm layer, 2 walls".to_string(),
                gcode_file: "".to_string(),
                print_type: "local".to_string(),
                ..Default::default()
            };
            // Should show the profile as-is without "Cloud:" prefix
            assert_eq!(status.display_name(), "0.2mm layer, 2 walls");
        }

        #[test]
        fn returns_empty_when_no_name() {
            let status = PrintStatus::default();
            assert_eq!(status.display_name(), "");
        }
    }

    mod looks_like_slicer_profile_tests {
        use super::*;

        #[test]
        fn detects_layer_pattern() {
            assert!(PrintStatus::looks_like_slicer_profile("0.2mm layer"));
            assert!(PrintStatus::looks_like_slicer_profile(
                "0.16mm Layer Height"
            ));
        }

        #[test]
        fn detects_infill_pattern() {
            assert!(PrintStatus::looks_like_slicer_profile("15% infill"));
            assert!(PrintStatus::looks_like_slicer_profile("20% Infill density"));
        }

        #[test]
        fn detects_walls_pattern() {
            assert!(PrintStatus::looks_like_slicer_profile("2 walls"));
            assert!(PrintStatus::looks_like_slicer_profile("3 Walls"));
        }

        #[test]
        fn detects_multiple_material_terms() {
            assert!(PrintStatus::looks_like_slicer_profile("PLA Draft"));
            assert!(PrintStatus::looks_like_slicer_profile("PETG Quality"));
        }

        #[test]
        fn allows_normal_project_names() {
            assert!(!PrintStatus::looks_like_slicer_profile("Benchy"));
            assert!(!PrintStatus::looks_like_slicer_profile("Phone Stand v2"));
            assert!(!PrintStatus::looks_like_slicer_profile("My Cool Model"));
        }

        #[test]
        fn single_material_term_is_not_profile() {
            // A single material mention shouldn't trigger profile detection
            // (requires 2+ profile terms)
            assert!(!PrintStatus::looks_like_slicer_profile("PLA"));
            assert!(!PrintStatus::looks_like_slicer_profile("My PETG Model"));
        }
    }

    mod update_from_message_tests {
        use super::*;

        #[test]
        fn preserves_unmentioned_fields() {
            let mut state = PrinterState::default();
            state.print_status.gcode_file = "existing.gcode".to_string();
            state.print_status.subtask_name = "My Project".to_string();

            // Update with message that only has progress
            let msg = MqttMessage {
                print: Some(PrintReport {
                    progress: Some(50),
                    ..Default::default()
                }),
                info: None,
            };
            state.update_from_message(&msg);

            // Original fields should be preserved
            assert_eq!(state.print_status.gcode_file, "existing.gcode");
            assert_eq!(state.print_status.subtask_name, "My Project");
            // New field should be updated
            assert_eq!(state.print_status.progress, 50);
        }

        #[test]
        fn updates_temperatures() {
            let mut state = PrinterState::default();

            let msg = MqttMessage {
                print: Some(PrintReport {
                    nozzle_temper: Some(215.5),
                    nozzle_target_temper: Some(220.0),
                    bed_temper: Some(60.0),
                    bed_target_temper: Some(65.0),
                    chamber_temper: Some(35.0),
                    ..Default::default()
                }),
                info: None,
            };
            state.update_from_message(&msg);

            assert_eq!(state.temperatures.nozzle, 215.5);
            assert_eq!(state.temperatures.nozzle_target, 220.0);
            assert_eq!(state.temperatures.bed, 60.0);
            assert_eq!(state.temperatures.bed_target, 65.0);
            assert_eq!(state.temperatures.chamber, 35.0);
        }

        #[test]
        fn updates_print_status_fields() {
            let mut state = PrinterState::default();

            let msg = MqttMessage {
                print: Some(PrintReport {
                    gcode_file: Some("test.gcode".to_string()),
                    subtask_name: Some("Test Print".to_string()),
                    progress: Some(75),
                    layer_num: Some(100),
                    total_layer_num: Some(200),
                    remaining_time: Some(45),
                    gcode_state: Some("RUNNING".to_string()),
                    print_type: Some("local".to_string()),
                    ..Default::default()
                }),
                info: None,
            };
            state.update_from_message(&msg);

            assert_eq!(state.print_status.gcode_file, "test.gcode");
            assert_eq!(state.print_status.subtask_name, "Test Print");
            assert_eq!(state.print_status.progress, 75);
            assert_eq!(state.print_status.layer_num, 100);
            assert_eq!(state.print_status.total_layers, 200);
            assert_eq!(state.print_status.remaining_time_mins, 45);
            assert_eq!(state.print_status.gcode_state, "RUNNING");
            assert_eq!(state.print_status.print_type, "local");
        }

        #[test]
        fn updates_fan_speeds() {
            let mut state = PrinterState::default();

            let msg = MqttMessage {
                print: Some(PrintReport {
                    cooling_fan_speed: Some("15".to_string()), // Max = 100%
                    big_fan1_speed: Some("7".to_string()),     // 47% (rounded)
                    big_fan2_speed: Some("0".to_string()),     // 0%
                    ..Default::default()
                }),
                info: None,
            };
            state.update_from_message(&msg);

            assert_eq!(state.speeds.fan_speed, 100);
            assert_eq!(state.speeds.aux_fan_speed, 47);
            assert_eq!(state.speeds.chamber_fan_speed, 0);
        }

        #[test]
        fn updates_lights() {
            let mut state = PrinterState::default();

            let msg = MqttMessage {
                print: Some(PrintReport {
                    lights_report: Some(vec![
                        LightReport {
                            node: "chamber_light".to_string(),
                            mode: "on".to_string(),
                        },
                        LightReport {
                            node: "work_light".to_string(),
                            mode: "off".to_string(),
                        },
                    ]),
                    ..Default::default()
                }),
                info: None,
            };
            state.update_from_message(&msg);

            assert!(state.lights.chamber_light);
            assert!(!state.lights.work_light);
        }

        #[test]
        fn parses_hms_errors() {
            let mut state = PrinterState::default();

            // attr layout: [module (8 bits)][severity (8 bits)][reserved (16 bits)]
            // module = (attr >> 24) & 0xFF
            // severity = (attr >> 16) & 0xFF
            let msg = MqttMessage {
                print: Some(PrintReport {
                    hms: Some(vec![
                        HmsReport {
                            attr: 0x0102_0000, // module 1, severity 2
                            code: 0x0700_0001, // AMS: Filament runout
                        },
                        HmsReport {
                            attr: 0x0201_0000, // module 2, severity 1
                            code: 0x9999_9999, // Unknown code
                        },
                    ]),
                    ..Default::default()
                }),
                info: None,
            };
            state.update_from_message(&msg);

            assert_eq!(state.hms_errors.len(), 2);
            assert_eq!(state.hms_errors[0].code, 0x0700_0001);
            assert_eq!(state.hms_errors[0].module, 1);
            assert_eq!(state.hms_errors[0].severity, 2);
            assert_eq!(state.hms_errors[0].message, "AMS: Filament runout");
            assert_eq!(state.hms_errors[1].module, 2);
            assert_eq!(state.hms_errors[1].severity, 1);
            assert_eq!(state.hms_errors[1].message, "See wiki.bambulab.com");
        }

        #[test]
        fn handles_empty_message() {
            let mut state = PrinterState::default();
            state.print_status.progress = 50;

            let msg = MqttMessage {
                print: None,
                info: None,
            };
            state.update_from_message(&msg);

            // State should be unchanged
            assert_eq!(state.print_status.progress, 50);
        }
    }

    mod ams_parsing_tests {
        use super::*;

        #[test]
        fn parses_active_tray_correctly() {
            let mut state = PrinterState::default();

            // tray_now "5" means unit 1 (5/4=1), tray 1 (5%4=1)
            let report = AmsReport {
                tray_now: Some("5".to_string()),
                ams: Some(vec![]),
                ..Default::default()
            };

            state.update_ams(&report);

            let ams = state.ams.as_ref().unwrap();
            assert_eq!(ams.current_unit, Some(1));
            assert_eq!(ams.current_tray, Some(1));
        }

        #[test]
        fn parses_first_unit_first_tray() {
            let mut state = PrinterState::default();

            // tray_now "0" means unit 0, tray 0
            let report = AmsReport {
                tray_now: Some("0".to_string()),
                ams: Some(vec![]),
                ..Default::default()
            };

            state.update_ams(&report);

            let ams = state.ams.as_ref().unwrap();
            assert_eq!(ams.current_unit, Some(0));
            assert_eq!(ams.current_tray, Some(0));
        }

        #[test]
        fn handles_external_spool() {
            let mut state = PrinterState::default();

            // tray_now "254" means external spool
            let report = AmsReport {
                tray_now: Some("254".to_string()),
                ams: Some(vec![]),
                ..Default::default()
            };

            state.update_ams(&report);

            let ams = state.ams.as_ref().unwrap();
            assert_eq!(ams.current_unit, None);
            assert_eq!(ams.current_tray, None);
        }

        #[test]
        fn handles_no_tray_selected() {
            let mut state = PrinterState::default();

            // tray_now "255" means no selection
            let report = AmsReport {
                tray_now: Some("255".to_string()),
                ams: Some(vec![]),
                ..Default::default()
            };

            state.update_ams(&report);

            let ams = state.ams.as_ref().unwrap();
            assert_eq!(ams.current_unit, None);
            assert_eq!(ams.current_tray, None);
        }

        #[test]
        fn parses_ams_units_and_trays() {
            let mut state = PrinterState::default();

            let report = AmsReport {
                tray_now: Some("0".to_string()),
                ams: Some(vec![AmsUnitReport {
                    id: "0".to_string(),
                    humidity: "4".to_string(),
                    tray: Some(vec![
                        AmsTrayReport {
                            id: "0".to_string(),
                            tray_type: Some("PLA".to_string()),
                            tray_color: Some("FF0000".to_string()),
                            remain: Some(85),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "1".to_string(),
                            tray_type: Some("PETG".to_string()),
                            tray_color: Some("#00FF00".to_string()),
                            remain: Some(50),
                            ..Default::default()
                        },
                    ]),
                }]),
                ..Default::default()
            };

            state.update_ams(&report);

            let ams = state.ams.as_ref().unwrap();
            assert_eq!(ams.units.len(), 1);

            let unit = &ams.units[0];
            assert_eq!(unit.id, 0);
            assert_eq!(unit.humidity, 4);
            assert_eq!(unit.trays.len(), 2);

            assert_eq!(unit.trays[0].material, "PLA");
            assert_eq!(unit.trays[0].remaining, 85);
            assert_eq!(unit.trays[0].parsed_color, Some((255, 0, 0)));

            assert_eq!(unit.trays[1].material, "PETG");
            assert_eq!(unit.trays[1].remaining, 50);
            assert_eq!(unit.trays[1].parsed_color, Some((0, 255, 0)));
        }

        #[test]
        fn detects_ams_lite() {
            let mut state = PrinterState::default();

            // AMS Lite has only 2 trays
            let report = AmsReport {
                tray_now: Some("0".to_string()),
                ams: Some(vec![AmsUnitReport {
                    id: "0".to_string(),
                    humidity: "0".to_string(),
                    tray: Some(vec![
                        AmsTrayReport {
                            id: "0".to_string(),
                            tray_type: Some("PLA".to_string()),
                            tray_color: None,
                            remain: Some(100),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "1".to_string(),
                            tray_type: Some("PLA".to_string()),
                            tray_color: None,
                            remain: Some(100),
                            ..Default::default()
                        },
                    ]),
                }]),
                ..Default::default()
            };

            state.update_ams(&report);

            let ams = state.ams.as_ref().unwrap();
            assert!(ams.units[0].is_lite);
        }

        #[test]
        fn full_ams_is_not_lite() {
            let mut state = PrinterState::default();

            // Full AMS has 4 trays
            let report = AmsReport {
                tray_now: Some("0".to_string()),
                ams: Some(vec![AmsUnitReport {
                    id: "0".to_string(),
                    humidity: "4".to_string(),
                    tray: Some(vec![
                        AmsTrayReport {
                            id: "0".to_string(),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "1".to_string(),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "2".to_string(),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "3".to_string(),
                            ..Default::default()
                        },
                    ]),
                }]),
                ..Default::default()
            };

            state.update_ams(&report);

            let ams = state.ams.as_ref().unwrap();
            assert!(!ams.units[0].is_lite);
        }

        #[test]
        fn handles_negative_remaining() {
            let mut state = PrinterState::default();

            // Negative remain values should be clamped to 0
            let report = AmsReport {
                tray_now: None,
                ams: Some(vec![AmsUnitReport {
                    id: "0".to_string(),
                    humidity: "3".to_string(),
                    tray: Some(vec![AmsTrayReport {
                        id: "0".to_string(),
                        tray_type: Some("PLA".to_string()),
                        tray_color: None,
                        remain: Some(-1),
                        ..Default::default()
                    }]),
                }]),
                ..Default::default()
            };

            state.update_ams(&report);

            let ams = state.ams.as_ref().unwrap();
            assert_eq!(ams.units[0].trays[0].remaining, 0);
        }
    }

    mod active_filament_type_tests {
        use super::*;

        #[test]
        fn returns_none_when_no_ams() {
            let state = PrinterState::default();
            assert_eq!(state.active_filament_type(), None);
        }

        #[test]
        fn returns_none_when_no_tray_selected() {
            let state = PrinterState {
                ams: Some(AmsState {
                    units: smallvec![AmsUnit {
                        id: 0,
                        humidity: 4,
                        trays: smallvec![AmsTray {
                            id: 0,
                            material: "PLA".to_string(),
                            remaining: 100,
                            ..Default::default()
                        }],
                        is_lite: false,
                    }],
                    current_unit: None,
                    current_tray: None,
                    ..Default::default()
                }),
                ..Default::default()
            };
            assert_eq!(state.active_filament_type(), None);
        }

        #[test]
        fn returns_material_when_tray_selected() {
            let state = PrinterState {
                ams: Some(AmsState {
                    units: smallvec![AmsUnit {
                        id: 0,
                        humidity: 4,
                        trays: smallvec![AmsTray {
                            id: 0,
                            material: "PETG".to_string(),
                            remaining: 85,
                            ..Default::default()
                        }],
                        is_lite: false,
                    }],
                    current_unit: Some(0),
                    current_tray: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            };
            assert_eq!(state.active_filament_type(), Some("PETG"));
        }

        #[test]
        fn returns_none_when_tray_empty() {
            let state = PrinterState {
                ams: Some(AmsState {
                    units: smallvec![AmsUnit {
                        id: 0,
                        humidity: 4,
                        trays: smallvec![AmsTray {
                            id: 0,
                            material: String::new(), // Empty tray
                            ..Default::default()
                        }],
                        is_lite: false,
                    }],
                    current_unit: Some(0),
                    current_tray: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            };
            assert_eq!(state.active_filament_type(), None);
        }

        #[test]
        fn handles_multi_unit_selection() {
            let state = PrinterState {
                ams: Some(AmsState {
                    units: smallvec![
                        AmsUnit {
                            id: 0,
                            humidity: 4,
                            trays: smallvec![AmsTray {
                                id: 0,
                                material: "PLA".to_string(),
                                remaining: 100,
                                ..Default::default()
                            }],
                            is_lite: false,
                        },
                        AmsUnit {
                            id: 1,
                            humidity: 3,
                            trays: smallvec![AmsTray {
                                id: 0,
                                material: "ABS".to_string(),
                                remaining: 50,
                                ..Default::default()
                            }],
                            is_lite: false,
                        },
                    ],
                    current_unit: Some(1), // Second unit selected
                    current_tray: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            };
            assert_eq!(state.active_filament_type(), Some("ABS"));
        }
    }

    mod is_active_tests {
        use super::*;

        #[test]
        fn running_state_is_active() {
            let status = PrintStatus {
                gcode_state: "RUNNING".to_string(),
                ..Default::default()
            };
            assert!(status.is_active());
        }

        #[test]
        fn pause_state_is_active() {
            let status = PrintStatus {
                gcode_state: "PAUSE".to_string(),
                ..Default::default()
            };
            assert!(status.is_active());
        }

        #[test]
        fn idle_state_is_not_active() {
            let status = PrintStatus {
                gcode_state: "IDLE".to_string(),
                ..Default::default()
            };
            assert!(!status.is_active());
        }

        #[test]
        fn finish_state_is_not_active() {
            let status = PrintStatus {
                gcode_state: "FINISH".to_string(),
                ..Default::default()
            };
            assert!(!status.is_active());
        }

        #[test]
        fn empty_state_is_not_active() {
            let status = PrintStatus::default();
            assert!(!status.is_active());
        }
    }

    /// Tests that deserialize JSON strings into MqttMessage, exercising the
    /// same code path as real MQTT messages from the printer. This catches
    /// type mismatches between JSON wire format and Rust struct definitions
    /// that struct-construction tests miss entirely.
    mod json_deserialization_tests {
        use super::*;

        /// Helper: deserialize JSON and assert it succeeds.
        fn parse_mqtt(json: &str) -> MqttMessage {
            serde_json::from_str::<MqttMessage>(json)
                .unwrap_or_else(|e| panic!("Failed to parse: {e}\nJSON: {json}"))
        }

        /// Helper: deserialize JSON, apply to state, return state.
        fn parse_and_apply(json: &str) -> PrinterState {
            let msg = parse_mqtt(json);
            let mut state = PrinterState::default();
            state.update_from_message(&msg);
            state
        }

        #[test]
        fn parses_minimal_status_message() {
            let state =
                parse_and_apply(r#"{"print": {"mc_percent": 42, "gcode_state": "RUNNING"}}"#);
            assert_eq!(state.print_status.progress, 42);
            assert_eq!(state.print_status.gcode_state, "RUNNING");
        }

        #[test]
        fn parses_temperatures_and_fans() {
            let state = parse_and_apply(
                r#"{"print": {
                    "nozzle_temper": 215.5,
                    "bed_temper": 60.0,
                    "chamber_temper": 35.2,
                    "cooling_fan_speed": "15",
                    "big_fan1_speed": "7",
                    "big_fan2_speed": "0"
                }}"#,
            );
            assert_eq!(state.temperatures.nozzle, 215.5);
            assert_eq!(state.temperatures.bed, 60.0);
            assert_eq!(state.temperatures.chamber, 35.2);
            assert_eq!(state.speeds.fan_speed, 100);
            assert_eq!(state.speeds.aux_fan_speed, 47);
            assert_eq!(state.speeds.chamber_fan_speed, 0);
        }

        #[test]
        fn parses_firmware_and_hardware_version() {
            let state =
                parse_and_apply(r#"{"print": {"hw_ver": "HW-1.2.3", "sw_ver": "01.08.02.00"}}"#);
            assert_eq!(state.hardware_version, "HW-1.2.3");
            assert_eq!(state.firmware_version, "01.08.02.00");
        }

        #[test]
        fn parses_nozzle_diameter() {
            let state = parse_and_apply(r#"{"print": {"nozzle_diameter": "0.4"}}"#);
            assert_eq!(state.nozzle_diameter, "0.4");
        }

        #[test]
        fn parses_heatbreak_fan_as_string() {
            let state = parse_and_apply(r#"{"print": {"heatbreak_fan_speed": "15"}}"#);
            assert_eq!(state.heatbreak_fan_speed, 100);
        }

        #[test]
        fn parses_heatbreak_fan_as_integer() {
            let state = parse_and_apply(r#"{"print": {"heatbreak_fan_speed": 15}}"#);
            assert_eq!(state.heatbreak_fan_speed, 100);
        }

        #[test]
        fn parses_gcode_start_time_as_number() {
            let state = parse_and_apply(r#"{"print": {"gcode_start_time": 1700000000}}"#);
            assert_eq!(state.gcode_start_time, Some(1_700_000_000));
        }

        #[test]
        fn parses_gcode_start_time_as_string() {
            let state = parse_and_apply(r#"{"print": {"gcode_start_time": "1700000000"}}"#);
            assert_eq!(state.gcode_start_time, Some(1_700_000_000));
        }

        #[test]
        fn ignores_zero_gcode_start_time() {
            let state = parse_and_apply(r#"{"print": {"gcode_start_time": 0}}"#);
            assert_eq!(state.gcode_start_time, None);
        }

        #[test]
        fn parses_xcam_with_bool_values() {
            let state = parse_and_apply(
                r#"{"print": {"xcam": {
                    "spaghetti_detector": true,
                    "first_layer_inspector": false,
                    "print_halt": true
                }}}"#,
            );
            assert!(state.xcam.spaghetti_detector);
            assert!(!state.xcam.first_layer_inspector);
            assert!(state.xcam.print_halt);
        }

        #[test]
        fn parses_xcam_with_integer_values() {
            let state = parse_and_apply(
                r#"{"print": {"xcam": {
                    "spaghetti_detector": 1,
                    "first_layer_inspector": 0,
                    "print_halt": 1
                }}}"#,
            );
            assert!(state.xcam.spaghetti_detector);
            assert!(!state.xcam.first_layer_inspector);
            assert!(state.xcam.print_halt);
        }

        #[test]
        fn parses_xcam_with_string_values() {
            let state = parse_and_apply(
                r#"{"print": {"xcam": {
                    "spaghetti_detector": "true",
                    "first_layer_inspector": "false",
                    "print_halt": "enable"
                }}}"#,
            );
            assert!(state.xcam.spaghetti_detector);
            assert!(!state.xcam.first_layer_inspector);
            assert!(state.xcam.print_halt);
        }

        #[test]
        fn parses_xcam_with_unknown_extra_fields() {
            // Printers may send fields we don't model
            let state = parse_and_apply(
                r#"{"print": {"xcam": {
                    "spaghetti_detector": true,
                    "buildplate_marker_detector": true,
                    "allow_skip_parts": false,
                    "halt_print_sensitivity": "medium",
                    "printing_monitor": true,
                    "some_future_field": 42
                }}}"#,
            );
            assert!(state.xcam.spaghetti_detector);
        }

        #[test]
        fn parses_ipcam() {
            let state = parse_and_apply(
                r#"{"print": {"ipcam": {
                    "ipcam_record": "enable",
                    "timelapse": "disable",
                    "resolution": "1080p"
                }}}"#,
            );
            assert!(state.ipcam.recording);
            assert!(!state.ipcam.timelapse);
            assert_eq!(state.ipcam.resolution, "1080p");
        }

        #[test]
        fn parses_ipcam_with_unknown_fields() {
            let state = parse_and_apply(
                r#"{"print": {"ipcam": {
                    "ipcam_record": "enable",
                    "ipcam_dev": "1",
                    "mode_bits": 3
                }}}"#,
            );
            assert!(state.ipcam.recording);
        }

        #[test]
        fn parses_lights_report() {
            let state = parse_and_apply(
                r#"{"print": {"lights_report": [
                    {"node": "chamber_light", "mode": "on"},
                    {"node": "work_light", "mode": "off"}
                ]}}"#,
            );
            assert!(state.lights.chamber_light);
            assert!(!state.lights.work_light);
        }

        #[test]
        fn parses_hms_errors() {
            let state = parse_and_apply(
                r#"{"print": {"hms": [
                    {"attr": 16908288, "code": 117440513}
                ]}}"#,
            );
            assert!(state.hms_received);
            assert_eq!(state.hms_errors.len(), 1);
        }

        #[test]
        fn parses_ams_with_tray_temps_as_strings() {
            let state = parse_and_apply(
                r#"{"print": {"ams": {
                    "tray_now": "0",
                    "ams": [{
                        "id": "0",
                        "humidity": "4",
                        "tray": [{
                            "id": "0",
                            "tray_type": "PLA",
                            "tray_color": "FF0000FF",
                            "remain": 85,
                            "tray_sub_brands": "Bambu PLA Basic",
                            "nozzle_temp_min": "190",
                            "nozzle_temp_max": "230"
                        }]
                    }]
                }}}"#,
            );
            let ams = state.ams.as_ref().unwrap();
            let tray = &ams.units[0].trays[0];
            assert_eq!(tray.material, "PLA");
            assert_eq!(tray.sub_brand, "Bambu PLA Basic");
            assert_eq!(tray.nozzle_temp_min, Some(190));
            assert_eq!(tray.nozzle_temp_max, Some(230));
        }

        #[test]
        fn ignores_unknown_top_level_fields() {
            // Printers send many fields we don't model
            let state = parse_and_apply(
                r#"{"print": {
                    "mc_percent": 50,
                    "mess_production_state": "active",
                    "sdcard": true,
                    "force_upgrade": false,
                    "lifecycle": "product"
                }}"#,
            );
            assert_eq!(state.print_status.progress, 50);
        }

        #[test]
        fn handles_empty_print_object() {
            let state = parse_and_apply(r#"{"print": {}}"#);
            assert_eq!(state.print_status.progress, 0);
        }

        #[test]
        fn handles_non_print_messages() {
            // Some MQTT messages have different top-level keys
            let msg = parse_mqtt(r#"{"info": {"command": "get_version"}}"#);
            assert!(msg.print.is_none());
        }

        #[test]
        fn parses_comprehensive_status_message() {
            // Simulates a realistic full pushall response
            let state = parse_and_apply(
                r#"{"print": {
                    "gcode_file": "benchy.gcode",
                    "subtask_name": "Benchy",
                    "mc_percent": 73,
                    "layer_num": 150,
                    "total_layer_num": 200,
                    "mc_remaining_time": 45,
                    "gcode_state": "RUNNING",
                    "print_type": "local",
                    "stg_cur": 0,
                    "nozzle_temper": 215.0,
                    "nozzle_target_temper": 220.0,
                    "bed_temper": 60.0,
                    "bed_target_temper": 60.0,
                    "chamber_temper": 38.0,
                    "spd_lvl": 2,
                    "cooling_fan_speed": "10",
                    "big_fan1_speed": "5",
                    "big_fan2_speed": "3",
                    "heatbreak_fan_speed": "12",
                    "gcode_start_time": 1700000000,
                    "nozzle_diameter": "0.4",
                    "hw_ver": "HW-1.0",
                    "sw_ver": "01.08.02.00",
                    "wifi_signal": "-45dBm",
                    "machine_name": "My Printer",
                    "lights_report": [
                        {"node": "chamber_light", "mode": "on"},
                        {"node": "work_light", "mode": "on"}
                    ],
                    "xcam": {
                        "spaghetti_detector": true,
                        "first_layer_inspector": true,
                        "print_halt": false
                    },
                    "ipcam": {
                        "ipcam_record": "enable",
                        "timelapse": "enable",
                        "resolution": "1080p"
                    },
                    "ams": {
                        "tray_now": "0",
                        "ams": [{
                            "id": "0",
                            "humidity": "3",
                            "tray": [{
                                "id": "0",
                                "tray_type": "PLA",
                                "tray_color": "00FF00FF",
                                "remain": 72,
                                "tray_sub_brands": "Bambu PLA Matte",
                                "nozzle_temp_min": "190",
                                "nozzle_temp_max": "230"
                            }]
                        }]
                    },
                    "hms": []
                }}"#,
            );
            assert_eq!(state.print_status.progress, 73);
            assert_eq!(state.print_status.gcode_file, "benchy.gcode");
            assert_eq!(state.print_status.layer_num, 150);
            assert_eq!(state.temperatures.nozzle, 215.0);
            assert_eq!(state.firmware_version, "01.08.02.00");
            assert_eq!(state.nozzle_diameter, "0.4");
            assert_eq!(state.heatbreak_fan_speed, 80); // 12/15 * 100
            assert_eq!(state.gcode_start_time, Some(1_700_000_000));
            assert!(state.xcam.spaghetti_detector);
            assert!(state.ipcam.recording);
            assert!(state.ipcam.timelapse);
            assert!(state.lights.chamber_light);
            assert!(state.lights.work_light);
            assert_eq!(state.printer_name, "My Printer");
            let tray = &state.ams.as_ref().unwrap().units[0].trays[0];
            assert_eq!(tray.sub_brand, "Bambu PLA Matte");
            assert_eq!(tray.nozzle_temp_min, Some(190));
            assert!(state.hms_received);
            assert!(state.hms_errors.is_empty());
        }
    }

    /// Tests for data-driven capability detection via ReceivedFields.
    mod capability_detection_tests {
        use super::*;

        #[test]
        fn no_capabilities_by_default() {
            let state = PrinterState::default();
            assert!(!state.has_chamber_temp_sensor());
            assert!(!state.has_heatbreak_fan());
            assert!(!state.has_xcam());
            assert!(!state.has_ipcam());
            assert!(!state.has_work_light());
            assert!(!state.has_aux_fan());
            assert!(!state.has_chamber_fan());
        }

        #[test]
        fn detects_chamber_temp_from_enclosed_model() {
            let mut state = PrinterState::default();
            state.set_model_from_serial("00M00A000000000"); // X1C
            assert!(state.has_chamber_temp_sensor());
        }

        #[test]
        fn no_chamber_temp_for_open_frame_model() {
            let mut state = PrinterState::default();
            state.set_model_from_serial("03900A000000000"); // A1
            assert!(!state.has_chamber_temp_sensor());
        }

        #[test]
        fn detects_heatbreak_fan() {
            let msg: MqttMessage =
                serde_json::from_str(r#"{"print": {"heatbreak_fan_speed": "10"}}"#).unwrap();
            let mut state = PrinterState::default();
            state.update_from_message(&msg);
            assert!(state.has_heatbreak_fan());
        }

        #[test]
        fn detects_xcam() {
            let msg: MqttMessage =
                serde_json::from_str(r#"{"print": {"xcam": {"spaghetti_detector": false}}}"#)
                    .unwrap();
            let mut state = PrinterState::default();
            state.update_from_message(&msg);
            assert!(state.has_xcam());
        }

        #[test]
        fn detects_ipcam() {
            let msg: MqttMessage =
                serde_json::from_str(r#"{"print": {"ipcam": {"ipcam_record": "disable"}}}"#)
                    .unwrap();
            let mut state = PrinterState::default();
            state.update_from_message(&msg);
            assert!(state.has_ipcam());
        }

        #[test]
        fn detects_work_light() {
            let msg: MqttMessage = serde_json::from_str(
                r#"{"print": {"lights_report": [{"node": "work_light", "mode": "off"}]}}"#,
            )
            .unwrap();
            let mut state = PrinterState::default();
            state.update_from_message(&msg);
            assert!(state.has_work_light());
        }

        #[test]
        fn chamber_light_does_not_set_work_light() {
            let msg: MqttMessage = serde_json::from_str(
                r#"{"print": {"lights_report": [{"node": "chamber_light", "mode": "on"}]}}"#,
            )
            .unwrap();
            let mut state = PrinterState::default();
            state.update_from_message(&msg);
            assert!(!state.has_work_light());
        }

        #[test]
        fn detects_aux_and_chamber_fans() {
            let msg: MqttMessage = serde_json::from_str(
                r#"{"print": {"big_fan1_speed": "5", "big_fan2_speed": "3"}}"#,
            )
            .unwrap();
            let mut state = PrinterState::default();
            state.update_from_message(&msg);
            assert!(state.has_aux_fan());
            assert!(state.has_chamber_fan());
        }

        #[test]
        fn capabilities_persist_across_updates() {
            let mut state = PrinterState::default();

            // First message has heatbreak fan
            let msg1: MqttMessage =
                serde_json::from_str(r#"{"print": {"heatbreak_fan_speed": "10"}}"#).unwrap();
            state.update_from_message(&msg1);
            assert!(state.has_heatbreak_fan());

            // Second message doesn't mention heatbreak fan
            let msg2: MqttMessage =
                serde_json::from_str(r#"{"print": {"mc_percent": 50}}"#).unwrap();
            state.update_from_message(&msg2);
            // Should still be detected
            assert!(state.has_heatbreak_fan());
        }
    }

    mod print_phase_tests {
        use super::*;

        fn make_running_status(stage_code: i32) -> PrintStatus {
            PrintStatus {
                gcode_state: "RUNNING".to_string(),
                stage_code,
                ..Default::default()
            }
        }

        #[test]
        fn returns_none_when_not_active() {
            let status = PrintStatus {
                gcode_state: "IDLE".to_string(),
                ..Default::default()
            };
            let temps = Temperatures::default();
            assert_eq!(status.print_phase(&temps), None);
        }

        #[test]
        fn detects_auto_leveling_from_stage() {
            let status = make_running_status(1);
            let temps = Temperatures::default();
            assert_eq!(status.print_phase(&temps), Some("Auto-Leveling"));
        }

        #[test]
        fn detects_bed_heating_from_stage() {
            let status = make_running_status(2);
            let temps = Temperatures::default();
            assert_eq!(status.print_phase(&temps), Some("Heating Bed"));
        }

        #[test]
        fn detects_nozzle_heating_from_stage() {
            let status = make_running_status(7);
            let temps = Temperatures::default();
            assert_eq!(status.print_phase(&temps), Some("Heating Nozzle"));
        }

        #[test]
        fn detects_cleaning_nozzle_from_stage() {
            let status = make_running_status(14);
            let temps = Temperatures::default();
            assert_eq!(status.print_phase(&temps), Some("Cleaning Nozzle"));
        }

        #[test]
        fn detects_homing_from_stage() {
            let status = make_running_status(13);
            let temps = Temperatures::default();
            assert_eq!(status.print_phase(&temps), Some("Homing"));
        }

        #[test]
        fn infers_bed_heating_from_temperature() {
            let status = PrintStatus {
                gcode_state: "RUNNING".to_string(),
                stage_code: 0,
                ..Default::default()
            };
            let temps = Temperatures {
                bed: 30.0,
                bed_target: 60.0,
                ..Default::default()
            };
            assert_eq!(status.print_phase(&temps), Some("Heating Bed"));
        }

        #[test]
        fn infers_nozzle_heating_from_temperature() {
            let status = PrintStatus {
                gcode_state: "RUNNING".to_string(),
                stage_code: 0,
                ..Default::default()
            };
            let temps = Temperatures {
                bed: 60.0,
                bed_target: 60.0,
                nozzle: 150.0,
                nozzle_target: 220.0,
                ..Default::default()
            };
            assert_eq!(status.print_phase(&temps), Some("Heating Nozzle"));
        }

        #[test]
        fn detects_printing_with_progress() {
            let status = PrintStatus {
                gcode_state: "RUNNING".to_string(),
                stage_code: 0,
                progress: 50,
                ..Default::default()
            };
            let temps = Temperatures {
                bed: 60.0,
                bed_target: 60.0,
                nozzle: 220.0,
                nozzle_target: 220.0,
                ..Default::default()
            };
            assert_eq!(status.print_phase(&temps), Some("Printing"));
        }

        #[test]
        fn detects_printing_with_layers() {
            let status = PrintStatus {
                gcode_state: "RUNNING".to_string(),
                stage_code: 0,
                layer_num: 5,
                ..Default::default()
            };
            let temps = Temperatures {
                bed: 60.0,
                bed_target: 60.0,
                nozzle: 220.0,
                nozzle_target: 220.0,
                ..Default::default()
            };
            assert_eq!(status.print_phase(&temps), Some("Printing"));
        }

        #[test]
        fn returns_preparing_as_fallback() {
            let status = PrintStatus {
                gcode_state: "RUNNING".to_string(),
                stage_code: 0,
                progress: 0,
                layer_num: 0,
                ..Default::default()
            };
            let temps = Temperatures {
                bed: 60.0,
                bed_target: 60.0,
                nozzle: 220.0,
                nozzle_target: 220.0,
                ..Default::default()
            };
            assert_eq!(status.print_phase(&temps), Some("Preparing"));
        }

        #[test]
        fn handles_pause_state() {
            let status = PrintStatus {
                gcode_state: "PAUSE".to_string(),
                stage_code: 16,
                ..Default::default()
            };
            let temps = Temperatures::default();
            assert_eq!(status.print_phase(&temps), Some("Paused"));
        }
    }

    mod parse_hex_bitmask_tests {
        use super::*;

        #[test]
        fn parses_valid_hex() {
            assert_eq!(parse_hex_bitmask("0F"), 0x0F);
            assert_eq!(parse_hex_bitmask("FF"), 0xFF);
            assert_eq!(parse_hex_bitmask("3C"), 0x3C);
        }

        #[test]
        fn parses_with_0x_prefix() {
            assert_eq!(parse_hex_bitmask("0x3C"), 0x3C);
            assert_eq!(parse_hex_bitmask("0XFF"), 0xFF);
        }

        #[test]
        fn parses_uppercase_and_lowercase() {
            assert_eq!(parse_hex_bitmask("ff"), 0xFF);
            assert_eq!(parse_hex_bitmask("FF"), 0xFF);
            assert_eq!(parse_hex_bitmask("aB"), 0xAB);
        }

        #[test]
        fn returns_zero_for_invalid_input() {
            assert_eq!(parse_hex_bitmask(""), 0);
            assert_eq!(parse_hex_bitmask("not_hex"), 0);
            assert_eq!(parse_hex_bitmask("GG"), 0);
        }

        #[test]
        fn parses_large_values() {
            assert_eq!(parse_hex_bitmask("FFFFFFFF"), 0xFFFF_FFFF);
            assert_eq!(parse_hex_bitmask("10000"), 0x10000);
        }
    }

    mod tray_bit_set_tests {
        use super::*;

        #[test]
        fn standard_unit_0_tray_0() {
            // Bit 0 set: unit 0, tray 0
            assert!(tray_bit_set(0b0001, 0, 0));
            assert!(!tray_bit_set(0b0001, 0, 1));
        }

        #[test]
        fn standard_unit_0_all_trays() {
            // Bits 0-3 set: unit 0, all 4 trays
            assert!(tray_bit_set(0b1111, 0, 0));
            assert!(tray_bit_set(0b1111, 0, 1));
            assert!(tray_bit_set(0b1111, 0, 2));
            assert!(tray_bit_set(0b1111, 0, 3));
        }

        #[test]
        fn standard_unit_1_trays() {
            // Unit 1 starts at bit 4
            // 0xF0 = 0b1111_0000 => unit 1, all trays
            assert!(tray_bit_set(0xF0, 1, 0));
            assert!(tray_bit_set(0xF0, 1, 1));
            assert!(tray_bit_set(0xF0, 1, 2));
            assert!(tray_bit_set(0xF0, 1, 3));
            // Unit 0 should not be set
            assert!(!tray_bit_set(0xF0, 0, 0));
        }

        #[test]
        fn ams_ht_unit_128_maps_to_offset_16() {
            // AMS HT (unit 128) tray 0 => bit 16
            assert!(tray_bit_set(1 << 16, AMS_HT_UNIT_ID, 0));
            assert!(!tray_bit_set(1 << 16, AMS_HT_UNIT_ID, 1));
            // AMS HT tray 1 => bit 17
            assert!(tray_bit_set(1 << 17, AMS_HT_UNIT_ID, 1));
        }

        #[test]
        fn out_of_range_returns_false() {
            // Standard unit 8 would need bit 32, which is out of u32 range
            assert!(!tray_bit_set(0xFFFF_FFFF, 8, 0));
            // AMS HT tray 16 would need bit 32
            assert!(!tray_bit_set(0xFFFF_FFFF, AMS_HT_UNIT_ID, 16));
        }

        #[test]
        fn zero_bitmask_always_false() {
            assert!(!tray_bit_set(0, 0, 0));
            assert!(!tray_bit_set(0, 1, 3));
            assert!(!tray_bit_set(0, AMS_HT_UNIT_ID, 0));
        }
    }

    mod ams_bitmask_integration_tests {
        use super::*;

        #[test]
        fn bitmask_fields_set_tray_booleans() {
            let mut state = PrinterState::default();

            // tray_exist_bits: 0x0F = all 4 trays exist in unit 0
            // tray_is_bbl_bits: 0x05 = trays 0 and 2 are BBL
            // tray_read_done_bits: 0x0F = all done
            // tray_reading_bits: 0x00 = none reading
            let report = AmsReport {
                tray_now: Some("0".to_string()),
                tray_exist_bits: Some("0F".to_string()),
                tray_is_bbl_bits: Some("05".to_string()),
                tray_read_done_bits: Some("0F".to_string()),
                tray_reading_bits: Some("00".to_string()),
                ams: Some(vec![AmsUnitReport {
                    id: "0".to_string(),
                    humidity: "4".to_string(),
                    tray: Some(vec![
                        AmsTrayReport {
                            id: "0".to_string(),
                            tray_type: Some("PLA".to_string()),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "1".to_string(),
                            tray_type: Some("PETG".to_string()),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "2".to_string(),
                            tray_type: Some("ABS".to_string()),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "3".to_string(),
                            ..Default::default()
                        },
                    ]),
                }]),
                ..Default::default()
            };

            state.update_ams(&report);

            let ams = state.ams.as_ref().unwrap();
            let trays = &ams.units[0].trays;

            // All 4 trays exist
            assert!(trays[0].tray_exists);
            assert!(trays[1].tray_exists);
            assert!(trays[2].tray_exists);
            assert!(trays[3].tray_exists);

            // Trays 0 and 2 are BBL (0x05 = 0b0101)
            assert!(trays[0].is_bbl);
            assert!(!trays[1].is_bbl);
            assert!(trays[2].is_bbl);
            assert!(!trays[3].is_bbl);

            // All done reading
            assert!(trays[0].read_done);
            assert!(trays[3].read_done);

            // None currently reading
            assert!(!trays[0].reading);
        }

        #[test]
        fn reading_bits_set_correctly() {
            let mut state = PrinterState::default();

            // Tray 1 is currently reading (bit 1)
            let report = AmsReport {
                tray_reading_bits: Some("02".to_string()),
                tray_exist_bits: Some("0F".to_string()),
                ams: Some(vec![AmsUnitReport {
                    id: "0".to_string(),
                    humidity: "4".to_string(),
                    tray: Some(vec![
                        AmsTrayReport {
                            id: "0".to_string(),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "1".to_string(),
                            ..Default::default()
                        },
                    ]),
                }]),
                ..Default::default()
            };

            state.update_ams(&report);

            let trays = &state.ams.as_ref().unwrap().units[0].trays;
            assert!(!trays[0].reading);
            assert!(trays[1].reading);
        }

        #[test]
        fn bitmask_values_persist_across_partial_updates() {
            let mut state = PrinterState::default();

            // First update: set bitmask values with tray data
            let report1 = AmsReport {
                tray_exist_bits: Some("0F".to_string()),
                tray_is_bbl_bits: Some("03".to_string()),
                ams: Some(vec![AmsUnitReport {
                    id: "0".to_string(),
                    humidity: "4".to_string(),
                    tray: Some(vec![
                        AmsTrayReport {
                            id: "0".to_string(),
                            tray_type: Some("PLA".to_string()),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "1".to_string(),
                            tray_type: Some("PETG".to_string()),
                            ..Default::default()
                        },
                    ]),
                }]),
                ..Default::default()
            };
            state.update_ams(&report1);

            let trays = &state.ams.as_ref().unwrap().units[0].trays;
            assert!(trays[0].tray_exists);
            assert!(trays[0].is_bbl);

            // Second update: only tray_now changes, no bitmask fields
            // Bitmask values should persist from cached state
            let report2 = AmsReport {
                tray_now: Some("1".to_string()),
                ams: Some(vec![AmsUnitReport {
                    id: "0".to_string(),
                    humidity: "4".to_string(),
                    tray: Some(vec![
                        AmsTrayReport {
                            id: "0".to_string(),
                            tray_type: Some("PLA".to_string()),
                            ..Default::default()
                        },
                        AmsTrayReport {
                            id: "1".to_string(),
                            tray_type: Some("PETG".to_string()),
                            ..Default::default()
                        },
                    ]),
                }]),
                ..Default::default()
            };
            state.update_ams(&report2);

            // Bitmask-derived booleans should persist
            let trays = &state.ams.as_ref().unwrap().units[0].trays;
            assert!(trays[0].tray_exists);
            assert!(trays[0].is_bbl);
            assert!(trays[1].tray_exists);
            assert!(trays[1].is_bbl);
        }

        #[test]
        fn json_with_bitmask_fields_deserializes() {
            let json = r#"{"print": {"ams": {
                "tray_now": "0",
                "ams_exist_bits": "01",
                "tray_exist_bits": "0F",
                "tray_is_bbl_bits": "05",
                "tray_read_done_bits": "0F",
                "tray_reading_bits": "00",
                "ams": [{
                    "id": "0",
                    "humidity": "4",
                    "tray": [{
                        "id": "0",
                        "tray_type": "PLA",
                        "tray_color": "FF0000FF",
                        "remain": 85
                    }]
                }]
            }}}"#;

            let msg: MqttMessage = serde_json::from_str(json).unwrap();
            let mut state = PrinterState::default();
            state.update_from_message(&msg);

            let ams = state.ams.as_ref().unwrap();
            let tray = &ams.units[0].trays[0];
            assert!(tray.tray_exists);
            assert!(tray.is_bbl);
            assert!(tray.read_done);
            assert!(!tray.reading);
        }
    }
}
