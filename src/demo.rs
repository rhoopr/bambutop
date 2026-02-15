//! Demo mode: pre-populated printer states for screenshots.
//!
//! Creates 3 demo printers with realistic data so the TUI can be
//! showcased without a real MQTT connection.

use crate::mqtt::SharedPrinterState;
use crate::printer::{
    AmsState, AmsTray, AmsUnit, HmsError, IpcamState, LightState, PrintStatus, PrinterState,
    ReceivedFields, Speeds, Temperatures, XcamState,
};
use smallvec::smallvec;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Number of seconds in one minute (for gcode_start_time calculations)
const SECS_PER_MINUTE: u64 = 60;

/// Creates the 3 demo printer states.
pub fn create_demo_printers() -> Vec<SharedPrinterState> {
    vec![
        Arc::new(Mutex::new(office_x1c())),
        Arc::new(Mutex::new(workshop_p1s())),
        Arc::new(Mutex::new(desk_a1_mini())),
    ]
}

/// Printer 1: Office X1C — actively printing "Benchy" at 75%.
fn office_x1c() -> PrinterState {
    let mut received = ReceivedFields::default();
    received.set(ReceivedFields::HEATBREAK_FAN);
    received.set(ReceivedFields::XCAM);
    received.set(ReceivedFields::IPCAM);
    received.set(ReceivedFields::WORK_LIGHT);
    received.set(ReceivedFields::AUX_FAN);
    received.set(ReceivedFields::CHAMBER_FAN);

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    PrinterState {
        connected: true,
        printer_name: "Office X1C".to_string(),
        printer_model: "X1 Carbon".to_string(),
        serial_suffix: "0M09".to_string(),
        print_status: PrintStatus {
            gcode_file: "Benchy.gcode.3mf".to_string(),
            subtask_name: "Benchy".to_string(),
            progress: 75,
            layer_num: 180,
            total_layers: 240,
            remaining_time_mins: 45,
            gcode_state: "RUNNING".to_string(),
            stage_code: 0,
            ..Default::default()
        },
        temperatures: Temperatures {
            nozzle: 250.0,
            nozzle_target: 250.0,
            bed: 60.0,
            bed_target: 60.0,
            chamber: 45.0,
        },
        speeds: Speeds {
            speed_level: 3,
            speed_magnitude: Some(140),
            fan_speed: 80,
            aux_fan_speed: 60,
            chamber_fan_speed: 50,
        },
        ams: Some(AmsState {
            units: smallvec![AmsUnit {
                id: 0,
                humidity: 3,
                trays: smallvec![
                    AmsTray {
                        id: 0,
                        material: "PLA".to_string(),
                        remaining: 85,
                        parsed_color: Some((220, 40, 40)),
                        sub_brand: "Bambu PLA Basic".to_string(),
                        nozzle_temp_min: Some(190),
                        nozzle_temp_max: Some(230),
                    },
                    AmsTray {
                        id: 1,
                        material: "PETG".to_string(),
                        remaining: 62,
                        parsed_color: Some((40, 100, 220)),
                        sub_brand: "Bambu PETG Basic".to_string(),
                        nozzle_temp_min: Some(230),
                        nozzle_temp_max: Some(260),
                    },
                    AmsTray {
                        id: 2,
                        material: "TPU".to_string(),
                        remaining: 40,
                        parsed_color: Some((40, 180, 80)),
                        sub_brand: "Bambu TPU 95A".to_string(),
                        nozzle_temp_min: Some(220),
                        nozzle_temp_max: Some(250),
                    },
                    AmsTray {
                        id: 3,
                        material: "ABS".to_string(),
                        remaining: 95,
                        parsed_color: Some((240, 240, 240)),
                        sub_brand: "Bambu ABS".to_string(),
                        nozzle_temp_min: Some(240),
                        nozzle_temp_max: Some(270),
                    },
                ],
                is_lite: false,
            }],
            current_tray: Some(0),
            current_unit: Some(0),
        }),
        lights: LightState {
            chamber_light: true,
            work_light: false,
        },
        wifi_signal: "-42dBm".to_string(),
        hms_errors: smallvec![],
        hms_received: true,
        firmware_version: "01.08.02.00".to_string(),
        hardware_version: "".to_string(),
        nozzle_diameter: "0.4".to_string(),
        heatbreak_fan_speed: 72,
        gcode_start_time: Some(now_unix - 45 * SECS_PER_MINUTE),
        xcam: XcamState {
            spaghetti_detector: true,
            first_layer_inspector: true,
            print_halt: false,
        },
        ipcam: IpcamState {
            recording: true,
            timelapse: true,
            resolution: "1080p".to_string(),
        },
        received,
    }
}

/// Printer 2: Workshop P1S — paused at 42% printing "Phone Stand".
fn workshop_p1s() -> PrinterState {
    let mut received = ReceivedFields::default();
    received.set(ReceivedFields::HEATBREAK_FAN);
    received.set(ReceivedFields::AUX_FAN);

    PrinterState {
        connected: true,
        printer_name: "Workshop P1S".to_string(),
        printer_model: "P1S".to_string(),
        serial_suffix: "1P07".to_string(),
        print_status: PrintStatus {
            gcode_file: "Phone Stand.gcode.3mf".to_string(),
            subtask_name: "Phone Stand".to_string(),
            progress: 42,
            layer_num: 85,
            total_layers: 200,
            remaining_time_mins: 87,
            gcode_state: "PAUSE".to_string(),
            stage_code: 16, // USER_PAUSED
            ..Default::default()
        },
        temperatures: Temperatures {
            nozzle: 180.0,
            nozzle_target: 0.0,
            bed: 40.0,
            bed_target: 0.0,
            chamber: 0.0,
        },
        speeds: Speeds {
            speed_level: 2,
            speed_magnitude: Some(100),
            fan_speed: 0,
            aux_fan_speed: 0,
            chamber_fan_speed: 0,
        },
        ams: Some(AmsState {
            units: smallvec![AmsUnit {
                id: 0,
                humidity: 5,
                trays: smallvec![
                    AmsTray {
                        id: 0,
                        material: "PLA".to_string(),
                        remaining: 70,
                        parsed_color: Some((255, 140, 0)),
                        sub_brand: "Bambu PLA Basic".to_string(),
                        nozzle_temp_min: Some(190),
                        nozzle_temp_max: Some(230),
                    },
                    AmsTray {
                        id: 1,
                        material: "PETG".to_string(),
                        remaining: 25,
                        parsed_color: Some((240, 240, 240)),
                        sub_brand: "Bambu PETG Basic".to_string(),
                        nozzle_temp_min: Some(230),
                        nozzle_temp_max: Some(260),
                    },
                    AmsTray {
                        id: 2,
                        material: String::new(),
                        remaining: 0,
                        parsed_color: None,
                        sub_brand: String::new(),
                        nozzle_temp_min: None,
                        nozzle_temp_max: None,
                    },
                    AmsTray {
                        id: 3,
                        material: String::new(),
                        remaining: 0,
                        parsed_color: None,
                        sub_brand: String::new(),
                        nozzle_temp_min: None,
                        nozzle_temp_max: None,
                    },
                ],
                is_lite: false,
            }],
            current_tray: Some(0),
            current_unit: Some(0),
        }),
        lights: LightState {
            chamber_light: false,
            work_light: false,
        },
        wifi_signal: "-58dBm".to_string(),
        hms_errors: smallvec![HmsError {
            code: 0x0500_0200,
            module: 5,
            severity: 2,
            message: "Filament may be tangled".to_string(),
            received_at: Instant::now(),
        }],
        hms_received: true,
        firmware_version: "01.07.06.00".to_string(),
        hardware_version: "".to_string(),
        nozzle_diameter: "0.4".to_string(),
        heatbreak_fan_speed: 0,
        gcode_start_time: None,
        xcam: XcamState::default(),
        ipcam: IpcamState::default(),
        received,
    }
}

/// Printer 3: Desk A1 Mini — idle, no job.
fn desk_a1_mini() -> PrinterState {
    PrinterState {
        connected: true,
        printer_name: "Desk A1 Mini".to_string(),
        printer_model: "A1 Mini".to_string(),
        serial_suffix: "3005".to_string(),
        wifi_signal: "-68dBm".to_string(),
        hms_received: true,
        firmware_version: "01.06.00.00".to_string(),
        nozzle_diameter: "0.4".to_string(),
        ..Default::default()
    }
}
