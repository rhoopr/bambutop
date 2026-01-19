use serde::Deserialize;
use std::borrow::Cow;

/// Special tray value indicating external spool (not in AMS).
/// Values >= this indicate no AMS tray is active (254=external, 255=none).
const TRAY_EXTERNAL_SPOOL: u8 = 254;

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
    pub hms_errors: Vec<HmsError>,
}

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
    pub units: Vec<AmsUnit>,
    /// The currently active tray slot (0-3 within a unit)
    pub current_tray: Option<u8>,
    /// The currently active AMS unit index (0-3)
    pub current_unit: Option<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct AmsUnit {
    pub id: u8,
    pub humidity: u8,
    pub trays: Vec<AmsTray>,
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
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct HmsError {
    pub code: u32,
    pub module: u8,
    pub severity: u8,
    pub message: String,
}

/// Raw MQTT message structure from Bambu printer
#[derive(Debug, Deserialize)]
pub struct MqttMessage {
    pub print: Option<PrintReport>,
}

/// Raw print report from MQTT containing all fields sent by the printer.
///
/// Many fields are not currently used by the application but are retained for:
/// - Documentation of the full Bambu MQTT protocol
/// - Future feature development without re-discovering field names
/// - Serde deserialization (unknown fields would otherwise cause errors)
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
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

    // HMS errors
    pub hms: Option<Vec<HmsReport>>,
}

#[derive(Debug, Deserialize)]
pub struct LightReport {
    pub node: String,
    pub mode: String,
}

#[derive(Debug, Deserialize)]
pub struct AmsReport {
    pub ams: Option<Vec<AmsUnitReport>>,
    pub tray_now: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AmsUnitReport {
    pub id: String,
    pub humidity: String,
    pub tray: Option<Vec<AmsTrayReport>>,
}

#[derive(Debug, Deserialize)]
pub struct AmsTrayReport {
    pub id: String,
    pub tray_type: Option<String>,
    pub tray_color: Option<String>,
    pub remain: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct HmsReport {
    pub attr: u32,
    pub code: u32,
}

impl PrinterState {
    pub fn update_from_message(&mut self, msg: &MqttMessage) {
        if let Some(print) = &msg.print {
            self.update_from_print_report(print);
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
            }
        }
        if let Some(v) = &report.big_fan2_speed {
            if let Some(speed) = parse_fan_speed(v) {
                self.speeds.chamber_fan_speed = speed;
            }
        }

        // Lights
        if let Some(lights) = &report.lights_report {
            for light in lights {
                match light.node.as_str() {
                    "chamber_light" => self.lights.chamber_light = light.mode == "on",
                    "work_light" => self.lights.work_light = light.mode == "on",
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
            self.hms_errors = hms_list
                .iter()
                .map(|h| HmsError {
                    code: h.code,
                    module: ((h.attr >> 24) & 0xFF) as u8,
                    severity: ((h.attr >> 16) & 0xFF) as u8,
                    message: format_hms_code(h.code).into_owned(),
                })
                .collect();
        }

        // Printer info
        if let Some(v) = &report.machine_name {
            self.printer_name.clone_from(v);
        }
    }

    /// Set model from serial number prefix
    pub fn set_model_from_serial(&mut self, serial: &str) {
        self.printer_model = model_from_serial(serial).to_string();
    }

    /// Returns true if this printer model has a chamber temperature sensor.
    /// X1 series, P2S, and H2 series have sensors.
    /// P1P, P1S, A1, and A1 Mini do not.
    pub fn has_chamber_temp_sensor(&self) -> bool {
        let model = self.printer_model.to_uppercase();
        model.contains("X1C")
            || model.contains("X1E")
            || model.contains("P2S")
            || model.contains("H2C")
            || model.contains("H2S")
            || model.contains("H2D")
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
                    ams_state.current_unit = Some(tray_val / 4);
                    ams_state.current_tray = Some(tray_val % 4);
                } else {
                    // External spool (254) or no selection (255)
                    ams_state.current_unit = None;
                    ams_state.current_tray = None;
                }
            }
        }

        if let Some(units) = &report.ams {
            ams_state.units = units
                .iter()
                .map(|u| {
                    let trays: Vec<AmsTray> = u
                        .tray
                        .as_ref()
                        .map(|trays| {
                            trays
                                .iter()
                                .map(|t| {
                                    let color_str = t.tray_color.as_deref().unwrap_or_default();
                                    AmsTray {
                                        id: t.id.parse().unwrap_or(0),
                                        material: t.tray_type.clone().unwrap_or_default(),
                                        remaining: t.remain.unwrap_or(0).max(0) as u8,
                                        parsed_color: parse_hex_color(color_str),
                                    }
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    // Detect AMS Lite: has only 2 tray slots instead of 4
                    // AMS Lite units report fewer trays or have humidity value of 0
                    let is_lite = trays.len() <= 2 && !trays.is_empty();

                    AmsUnit {
                        id: u.id.parse().unwrap_or(0),
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
    let val: u8 = s.parse().ok()?;
    // Bambu uses 0-15 scale, convert to percentage
    // Cap at 15 to prevent overflow in edge cases
    let capped = val.min(15);
    Some(((capped as f32 / 15.0) * 100.0) as u8)
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
        _ => Cow::Owned(format!("Error: 0x{:08X}", code)),
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
