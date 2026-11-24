//! 具身硬件终端附身中枢（`Hardware Embodiment Hub` · `FayGer Hardware Bridge Protocol 1.0 (FHBP/1.0)`）。
//!
//! 统一管理两类硬件终端接入层级与四大物理安全不变量：
//! - **Tier A（嵌入式 Linux / ROS2 伴机）**：无人机（`drone_px4_01`）、六轴协作机器人（`robot_arm_6dof_01`）；
//! - **Tier B（MCU / 工业控制器 C99 `FHBP/1.0` HTTP/TCP 桥接）**：遥控赛车（`rccar_ackermann_01`）、
//!   五轴数控机床（`cnc_mill_5axis_01`）、电影摄影机摇臂（`camera_jib_pro_01`）以及外部实时接入的 C99 固件节点；
//! - **`Faying` 监护守卫**：未附身或处于 `ROGUE` 态的分身发起任何驱动指令一律返回 `403 Forbidden`；
//! - **`CAP` 物理安全包络（Physical Safety Envelope）**：每次驱动执行前确定性校验全部数值参数上下界，
//!   越界指令（如无人机 `300m > 120m`、CNC 主轴 `25000 RPM > 12000 RPM`）立即以 `400 PHYSICAL_ENVELOPE_EXCEEDED` 拦截并记入审计链；
//! - **高危工序 `CAP` 人工审批**：如五轴机床 `start_spindle` 在通过包络检查后进入 `CAP` 待批列表，经 `Human Prime` 批准后方可执行；
//! - **`G0` 全链路硬件急刹级联**：一旦触发 `G0` 红色急刹，附身硬件立即切入各自物理安全态（`LOITER_HOVER_RTL`、
//!   `NEUTRAL_FULL_BRAKE`、`SAFE_TORQUE_HOLD`、`E_STOP_SPINDLE_OFF`、`DAMPED_SOFT_LOCK`），清零危险执行器，并向外部 C99 节点发送 `POST /fhbp/v1/g0_halt`。

use crate::error::CliError;
use crate::wallet::unix_now_secs;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

/// `FayGer Hardware Bridge Protocol` 规范版本号。
pub const FHBP_PROTOCOL_VERSION: &str = "fhbp/1.0";

/// 物理安全包络越界错误码常量（由守护进程映射为 `400 Bad Request` JSON `code` 字段）。
pub const PHYSICAL_ENVELOPE_EXCEEDED_CODE: &str = "PHYSICAL_ENVELOPE_EXCEEDED";

/// 单项物理量安全包络上下界。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeBound {
    /// 物理允许下界（含）。
    pub min: f64,
    /// 物理允许上界（含）。
    pub max: f64,
    /// 物理量单位说明（如 `"m"`、`"m/s"`、`"deg"`、`"RPM"`、`"%"`）。
    #[serde(default)]
    pub unit: String,
}

impl EnvelopeBound {
    fn new(min: f64, max: f64, unit: &str) -> Self {
        Self {
            min,
            max,
            unit: unit.to_owned(),
        }
    }
}

/// 单个具身硬件终端（内置数字孪生参考终端或外部 `FHBP/1.0` C99 固件节点）持久化记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareDeviceRecord {
    /// 硬件终端唯一标识（如 `"drone_px4_01"`、`"cnc_mill_5axis_01"` 或外部固件声明 ID）。
    pub device_id: String,
    /// 人类可读显示名称。
    pub display_name: String,
    /// 硬件品类（`"drone"` | `"rc_car"` | `"robot"` | `"cnc_machine"` | `"camera_jib"` | `"custom"`）。
    pub category: String,
    /// 接入架构层级（`"tier_a_embedded_linux"` | `"tier_a_ros2_linux"` | `"tier_b_c99_bridge"` | `"tier_b_industrial_bridge"`）。
    pub integration_tier: String,
    /// `FHBP` 协议版本。
    #[serde(default = "default_fhbp_version")]
    pub protocol_version: String,
    /// `G0` 急刹时强制切入的硬件安全态名称（如 `"LOITER_HOVER_RTL"` / `"E_STOP_SPINDLE_OFF"`）。
    pub safe_state_name: String,
    /// 当前物理运行态（`"READY"` | `"ACTUATING"` | `<safe_state_name>`）。
    pub current_state: String,
    /// 当前附身控制该硬件的 `Fay` 标识（未附身时为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embodied_fay_id: Option<String>,
    /// 外部 `FHBP/1.0` C99/嵌入式桥接节点 HTTP 端点（内置数字孪生为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_endpoint: Option<String>,
    /// 该终端支持的动作名列表。
    #[serde(default)]
    pub supported_actions: Vec<String>,
    /// 即使在安全包络内仍必须经 `Human Prime` `CAP` 二次确认的高危动作列表（如 CNC `"start_spindle"`）。
    #[serde(default)]
    pub requires_cap_approval_for: Vec<String>,
    /// 确定性物理安全包络（参数名 -> 上下界）。
    pub envelope: BTreeMap<String, EnvelopeBound>,
    /// 最新实时遥测键值对。
    pub telemetry: Value,
    /// 最近一次执行的动作摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_action_summary: Option<String>,
    /// 最近状态更新时间（Unix 秒）。
    pub updated_at_unix: u64,
}

fn default_fhbp_version() -> String {
    FHBP_PROTOCOL_VERSION.to_owned()
}

/// `POST /api/v1/hardware/devices/<device_id>/actuate` 响应载荷。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareActuateReceipt {
    /// 执行状态：`"actuated"` 或 `"pending_cap_approval"`。
    pub status: String,
    /// 若挂起等待 `CAP` 人工审批，返回审批单号（`apr_...`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    /// 人类可读执行或挂起摘要。
    pub message: String,
    /// 最新硬件终端状态快照。
    pub device: HardwareDeviceRecord,
}

/// `POST /api/v1/hardware/g0_halt` / `g0_all` 全硬件安全停机回执。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HardwareG0HaltReceipt {
    /// 已切入各自物理安全态的硬件设备 ID 列表。
    pub halted_devices: Vec<String>,
    /// 已通知远端 `POST /fhbp/v1/g0_halt` 的外部桥接设备 ID 列表。
    pub remote_bridges_notified: Vec<String>,
    /// 同步冻结退回 `ROGUE` 的分身 ID 列表。
    pub frozen_fays: Vec<String>,
    /// 急刹原因。
    pub reason: String,
}

/// 生成预置的五大标杆硬件终端集合（无人机、遥控赛车、六轴协作机器人、五轴数控机床、电影摄影机摇臂）。
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn default_reference_hardware_devices() -> BTreeMap<String, HardwareDeviceRecord> {
    let now = unix_now_secs();
    let mut map = BTreeMap::new();

    // 1. 无人机 · Quadrotor Companion (Tier A Embedded Linux)
    let mut drone_env = BTreeMap::new();
    drone_env.insert("altitude_m".to_owned(), EnvelopeBound::new(1.0, 120.0, "m"));
    drone_env.insert("speed_mps".to_owned(), EnvelopeBound::new(0.0, 12.0, "m/s"));
    drone_env.insert(
        "min_battery_pct".to_owned(),
        EnvelopeBound::new(20.0, 100.0, "%"),
    );
    map.insert(
        "drone_px4_01".to_owned(),
        HardwareDeviceRecord {
            device_id: "drone_px4_01".to_owned(),
            display_name: "无人机 · Quadrotor Companion".to_owned(),
            category: "drone".to_owned(),
            integration_tier: "tier_a_embedded_linux".to_owned(),
            protocol_version: FHBP_PROTOCOL_VERSION.to_owned(),
            safe_state_name: "LOITER_HOVER_RTL".to_owned(),
            current_state: "READY".to_owned(),
            embodied_fay_id: None,
            bridge_endpoint: None,
            supported_actions: vec![
                "takeoff".to_owned(),
                "goto_waypoint".to_owned(),
                "orbit_poi".to_owned(),
                "land".to_owned(),
            ],
            requires_cap_approval_for: Vec::new(),
            envelope: drone_env,
            telemetry: json!({
                "altitude_m": 0.0,
                "speed_mps": 0.0,
                "battery_pct": 88.0,
                "gps_lock": "RTK_FIXED"
            }),
            last_action_summary: None,
            updated_at_unix: now,
        },
    );

    // 2. 遥控赛车 · Ackermann Chassis (Tier B C99 Bridge)
    let mut rccar_env = BTreeMap::new();
    rccar_env.insert(
        "steering_deg".to_owned(),
        EnvelopeBound::new(-35.0, 35.0, "deg"),
    );
    rccar_env.insert(
        "throttle_pct".to_owned(),
        EnvelopeBound::new(0.0, 60.0, "%"),
    );
    map.insert(
        "rccar_ackermann_01".to_owned(),
        HardwareDeviceRecord {
            device_id: "rccar_ackermann_01".to_owned(),
            display_name: "遥控赛车 · Ackermann Chassis".to_owned(),
            category: "rc_car".to_owned(),
            integration_tier: "tier_b_c99_bridge".to_owned(),
            protocol_version: FHBP_PROTOCOL_VERSION.to_owned(),
            safe_state_name: "NEUTRAL_FULL_BRAKE".to_owned(),
            current_state: "READY".to_owned(),
            embodied_fay_id: None,
            bridge_endpoint: None,
            supported_actions: vec!["drive_control".to_owned(), "brake".to_owned()],
            requires_cap_approval_for: Vec::new(),
            envelope: rccar_env,
            telemetry: json!({
                "steering_deg": 0.0,
                "throttle_pct": 0.0,
                "wheel_rpm": 0,
                "pack_voltage_v": 7.9
            }),
            last_action_summary: None,
            updated_at_unix: now,
        },
    );

    // 3. 协作机器人 · 6-DoF Manipulator (Tier A ROS2 Linux)
    let mut robot_env = BTreeMap::new();
    robot_env.insert(
        "reach_mm".to_owned(),
        EnvelopeBound::new(100.0, 850.0, "mm"),
    );
    robot_env.insert(
        "joint_speed_dps".to_owned(),
        EnvelopeBound::new(1.0, 90.0, "deg/s"),
    );
    robot_env.insert(
        "torque_limit_nm".to_owned(),
        EnvelopeBound::new(1.0, 15.0, "N·m"),
    );
    map.insert(
        "robot_arm_6dof_01".to_owned(),
        HardwareDeviceRecord {
            device_id: "robot_arm_6dof_01".to_owned(),
            display_name: "协作机器人 · 6-DoF Manipulator".to_owned(),
            category: "robot".to_owned(),
            integration_tier: "tier_a_ros2_linux".to_owned(),
            protocol_version: FHBP_PROTOCOL_VERSION.to_owned(),
            safe_state_name: "SAFE_TORQUE_HOLD".to_owned(),
            current_state: "READY".to_owned(),
            embodied_fay_id: None,
            bridge_endpoint: None,
            supported_actions: vec![
                "move_pose".to_owned(),
                "gripper_actuate".to_owned(),
                "home_axes".to_owned(),
            ],
            requires_cap_approval_for: Vec::new(),
            envelope: robot_env,
            telemetry: json!({
                "reach_mm": 320.0,
                "joint_speed_dps": 0.0,
                "torque_limit_nm": 6.0,
                "gripper_open": true
            }),
            last_action_summary: None,
            updated_at_unix: now,
        },
    );

    // 4. 五轴数控机床 · 5-Axis CNC Mill (Tier B Industrial Bridge · start_spindle 需 CAP 人类原型确认)
    let mut cnc_env = BTreeMap::new();
    cnc_env.insert(
        "spindle_rpm".to_owned(),
        EnvelopeBound::new(0.0, 12000.0, "RPM"),
    );
    cnc_env.insert(
        "feed_mm_min".to_owned(),
        EnvelopeBound::new(0.0, 3000.0, "mm/min"),
    );
    cnc_env.insert(
        "z_depth_mm".to_owned(),
        EnvelopeBound::new(-200.0, 0.0, "mm"),
    );
    map.insert(
        "cnc_mill_5axis_01".to_owned(),
        HardwareDeviceRecord {
            device_id: "cnc_mill_5axis_01".to_owned(),
            display_name: "五轴数控机床 · 5-Axis CNC Mill".to_owned(),
            category: "cnc_machine".to_owned(),
            integration_tier: "tier_b_industrial_bridge".to_owned(),
            protocol_version: FHBP_PROTOCOL_VERSION.to_owned(),
            safe_state_name: "E_STOP_SPINDLE_OFF".to_owned(),
            current_state: "READY".to_owned(),
            embodied_fay_id: None,
            bridge_endpoint: None,
            supported_actions: vec!["jog_axis".to_owned(), "start_spindle".to_owned()],
            requires_cap_approval_for: vec!["start_spindle".to_owned()],
            envelope: cnc_env,
            telemetry: json!({
                "spindle_rpm": 0.0,
                "feed_mm_min": 0.0,
                "z_depth_mm": 0.0,
                "coolant": "STANDBY"
            }),
            last_action_summary: None,
            updated_at_unix: now,
        },
    );

    // 5. 电影摄影机摇臂 · Cinema Motion Jib (Tier B C99 Bridge)
    let mut jib_env = BTreeMap::new();
    jib_env.insert(
        "pan_deg".to_owned(),
        EnvelopeBound::new(-180.0, 180.0, "deg"),
    );
    jib_env.insert(
        "tilt_deg".to_owned(),
        EnvelopeBound::new(-45.0, 60.0, "deg"),
    );
    jib_env.insert(
        "angular_speed_dps".to_owned(),
        EnvelopeBound::new(1.0, 45.0, "deg/s"),
    );
    map.insert(
        "camera_jib_pro_01".to_owned(),
        HardwareDeviceRecord {
            device_id: "camera_jib_pro_01".to_owned(),
            display_name: "电影摄影机摇臂 · Cinema Motion Jib".to_owned(),
            category: "camera_jib".to_owned(),
            integration_tier: "tier_b_c99_bridge".to_owned(),
            protocol_version: FHBP_PROTOCOL_VERSION.to_owned(),
            safe_state_name: "DAMPED_SOFT_LOCK".to_owned(),
            current_state: "READY".to_owned(),
            embodied_fay_id: None,
            bridge_endpoint: None,
            supported_actions: vec!["sweep_keyframe".to_owned(), "park_boom".to_owned()],
            requires_cap_approval_for: Vec::new(),
            envelope: jib_env,
            telemetry: json!({
                "pan_deg": 0.0,
                "tilt_deg": 0.0,
                "angular_speed_dps": 0.0,
                "damping_lock": false
            }),
            last_action_summary: None,
            updated_at_unix: now,
        },
    );

    map
}

/// 确定性校验动作与全部数值参数是否满足硬件终端的 `CAP` 物理安全包络（Physical Safety Envelope）。
///
/// 若任一参数超出 `[min, max]`（或无人机电池低于 `min_battery_pct`），返回以 `PHYSICAL_ENVELOPE_EXCEEDED` 开头的错误描述。
pub fn validate_physical_envelope(
    device: &HardwareDeviceRecord,
    action: &str,
    params: &Value,
) -> Result<(), String> {
    let clean_action = action.trim();
    if clean_action.is_empty() {
        return Err("missing hardware action name".to_owned());
    }
    if !device.supported_actions.is_empty()
        && !device.supported_actions.iter().any(|a| a == clean_action)
    {
        return Err(format!(
            "unsupported action '{clean_action}' on '{}' (supported: [{}])",
            device.device_id,
            device.supported_actions.join(", ")
        ));
    }

    // 检查无人机最低电量包络（若包络定义了 min_battery_pct 且遥测或参数中当前电量低于下限）
    if let Some(batt_bound) = device.envelope.get("min_battery_pct") {
        if clean_action != "land" {
            if let Some(cur_batt) = params
                .get("battery_pct")
                .and_then(Value::as_f64)
                .or_else(|| device.telemetry.get("battery_pct").and_then(Value::as_f64))
            {
                if cur_batt < batt_bound.min {
                    return Err(format!(
                        "{PHYSICAL_ENVELOPE_EXCEEDED_CODE}: battery_pct={cur_batt} is below minimum safety threshold {} {} on {}",
                        batt_bound.min, batt_bound.unit, device.device_id
                    ));
                }
            }
        }
    }

    if let Some(obj) = params.as_object() {
        for (key, val) in obj {
            if let Some(num) = val.as_f64() {
                if !num.is_finite() {
                    return Err(format!(
                        "{PHYSICAL_ENVELOPE_EXCEEDED_CODE}: parameter '{key}' must be a finite number"
                    ));
                }
                if let Some(bound) = device.envelope.get(key) {
                    if num < bound.min || num > bound.max {
                        return Err(format!(
                            "{PHYSICAL_ENVELOPE_EXCEEDED_CODE}: parameter '{key}' = {num} violates physical safety envelope [{}, {}] {} on '{}'",
                            bound.min, bound.max, bound.unit, device.device_id
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// 在内置数字孪生（或远端返回后）应用合法动作并刷新实时遥测。
pub fn apply_local_twin_actuation(
    device: &mut HardwareDeviceRecord,
    action: &str,
    params: &Value,
) -> String {
    let clean_action = action.trim();
    let now = unix_now_secs();
    device.updated_at_unix = now;

    if matches!(
        clean_action,
        "land" | "brake" | "home_axes" | "park_boom" | "halt"
    ) {
        "READY".clone_into(&mut device.current_state);
        zero_hazardous_telemetry(&mut device.telemetry);
        if clean_action == "land" {
            if let Some(m) = device.telemetry.as_object_mut() {
                m.insert("altitude_m".to_owned(), json!(0.0));
            }
        } else if clean_action == "home_axes" {
            if let Some(m) = device.telemetry.as_object_mut() {
                m.insert("reach_mm".to_owned(), json!(250.0));
            }
        } else if clean_action == "park_boom" {
            if let Some(m) = device.telemetry.as_object_mut() {
                m.insert("pan_deg".to_owned(), json!(0.0));
                m.insert("tilt_deg".to_owned(), json!(0.0));
            }
        }
    } else {
        "ACTUATING".clone_into(&mut device.current_state);
        if let (Some(tel_map), Some(param_map)) =
            (device.telemetry.as_object_mut(), params.as_object())
        {
            for (k, v) in param_map {
                tel_map.insert(k.clone(), v.clone());
            }
        }
    }

    let summary = format!(
        "Executed FHBP/1.0 '{}' on {} -> state={} telemetry={}",
        clean_action, device.device_id, device.current_state, device.telemetry
    );
    device.last_action_summary = Some(summary.clone());
    summary
}

/// 将单个硬件终端强制切入其专属物理安全态并清零危险执行器。
pub fn transition_device_to_g0_safe_state(device: &mut HardwareDeviceRecord, reason: &str) {
    device.safe_state_name.clone_into(&mut device.current_state);
    zero_hazardous_telemetry(&mut device.telemetry);
    device.updated_at_unix = unix_now_secs();
    device.last_action_summary = Some(format!(
        "G0 SAFE HALT -> {} ({reason})",
        device.safe_state_name
    ));
}

fn zero_hazardous_telemetry(telemetry: &mut Value) {
    if let Some(map) = telemetry.as_object_mut() {
        for key in [
            "speed_mps",
            "throttle_pct",
            "wheel_rpm",
            "joint_speed_dps",
            "spindle_rpm",
            "feed_mm_min",
            "angular_speed_dps",
        ] {
            if map.contains_key(key) {
                map.insert(key.to_owned(), json!(0.0));
            }
        }
    }
}

/// 校验并规范化本机回环或局域网 `FHBP/1.0` 硬件桥接 HTTP 端点（如 `"http://127.0.0.1:9101"` 或 `"http://192.168.1.50:9100"`）。
pub fn normalize_fhbp_endpoint(raw: &str) -> Result<String, CliError> {
    let trimmed = raw.trim().trim_end_matches('/');
    let Some(without_scheme) = trimmed.strip_prefix("http://") else {
        return Err(CliError::Usage(format!(
            "invalid FHBP hardware endpoint '{trimmed}': must start with http:// (loopback or local LAN)"
        )));
    };
    if without_scheme.is_empty()
        || without_scheme.contains('@')
        || without_scheme.chars().any(char::is_whitespace)
    {
        return Err(CliError::Usage(format!(
            "invalid FHBP hardware endpoint '{trimmed}'"
        )));
    }
    let authority = without_scheme.split('/').next().unwrap_or("");
    let host = if authority.starts_with('[') {
        authority
            .split(']')
            .next()
            .map_or("", |s| s.trim_start_matches('['))
    } else {
        authority.split(':').next().unwrap_or("")
    };
    if !is_permitted_hardware_bridge_host(host) {
        return Err(CliError::Usage(format!(
            "FHBP hardware endpoint host '{host}' rejected: only loopback (127.0.0.1/localhost) or private LAN IPv4 addresses are allowed"
        )));
    }
    Ok(format!("http://{authority}"))
}

fn is_permitted_hardware_bridge_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") || host == "::1" {
        return true;
    }
    let parts: Vec<u8> = host
        .split('.')
        .filter_map(|seg| seg.parse::<u8>().ok())
        .collect();
    if parts.len() != 4 {
        return false;
    }
    match (parts[0], parts[1]) {
        (127 | 10, _) | (192, 168) | (169, 254) => true,
        (172, b) if (16..=31).contains(&b) => true,
        _ => false,
    }
}

/// 连接外部 `FHBP/1.0` C99/嵌入式固件节点，拉取 `GET <endpoint>/fhbp/v1/manifest` 并构造 [`HardwareDeviceRecord`]。
pub fn fhbp_fetch_remote_manifest(
    endpoint: &str,
    existing_embodied_fay: Option<String>,
) -> Result<HardwareDeviceRecord, CliError> {
    let clean_ep = normalize_fhbp_endpoint(endpoint)?;
    let val = fhbp_http_call(&clean_ep, "GET", "/fhbp/v1/manifest", None)?;

    let device_id = val
        .get("device_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            CliError::Usage(
                "remote FHBP /fhbp/v1/manifest missing non-empty 'device_id'".to_owned(),
            )
        })?
        .to_owned();

    let display_name = val
        .get("display_name")
        .or_else(|| val.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&device_id)
        .to_owned();

    let category = val
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or("custom_hardware")
        .to_owned();
    let integration_tier = val
        .get("integration_tier")
        .or_else(|| val.get("tier"))
        .and_then(Value::as_str)
        .unwrap_or("tier_b_c99_bridge")
        .to_owned();
    let safe_state_name = val
        .get("safe_state_name")
        .or_else(|| val.get("safe_state"))
        .and_then(Value::as_str)
        .unwrap_or("G0_SAFE_HALT")
        .to_owned();
    let current_state = val
        .get("current_state")
        .or_else(|| val.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("READY")
        .to_owned();

    let supported_actions = parse_str_vec_field(&val, "supported_actions");
    let requires_cap_approval_for = parse_str_vec_field(&val, "requires_cap_approval_for");
    let envelope = parse_envelope_map(val.get("envelope"));
    let telemetry = val
        .get("telemetry")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));

    Ok(HardwareDeviceRecord {
        device_id,
        display_name,
        category,
        integration_tier,
        protocol_version: FHBP_PROTOCOL_VERSION.to_owned(),
        safe_state_name,
        current_state,
        embodied_fay_id: existing_embodied_fay,
        bridge_endpoint: Some(clean_ep.clone()),
        supported_actions,
        requires_cap_approval_for,
        envelope,
        telemetry,
        last_action_summary: Some(format!("Connected live FHBP/1.0 node at {clean_ep}")),
        updated_at_unix: unix_now_secs(),
    })
}

/// 向外部 `FHBP/1.0` C99/嵌入式固件节点转发已通过 `Faying + CAP` 门禁的驱动请求（`POST <endpoint>/fhbp/v1/actuate`）。
pub fn fhbp_forward_actuate(
    endpoint: &str,
    device: &mut HardwareDeviceRecord,
    fay_id: &str,
    action: &str,
    params: &Value,
) -> Result<String, CliError> {
    let payload = json!({
        "protocol": FHBP_PROTOCOL_VERSION,
        "device_id": device.device_id,
        "fay_id": fay_id,
        "action": action,
        "params": params,
    });
    let resp = fhbp_http_call(endpoint, "POST", "/fhbp/v1/actuate", Some(&payload))?;

    if let Some(st) = resp
        .get("current_state")
        .or_else(|| resp.get("state"))
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    {
        st.clone_into(&mut device.current_state);
    } else {
        "ACTUATING".clone_into(&mut device.current_state);
    }

    if let Some(tel) = resp.get("telemetry") {
        device.telemetry = tel.clone();
    } else if let (Some(tel_map), Some(param_map)) =
        (device.telemetry.as_object_mut(), params.as_object())
    {
        for (k, v) in param_map {
            tel_map.insert(k.clone(), v.clone());
        }
    }
    device.updated_at_unix = unix_now_secs();
    let msg = resp.get("message").and_then(Value::as_str).map_or_else(
        || {
            format!(
                "Forwarded FHBP/1.0 '{action}' to {} ({endpoint}) -> state={}",
                device.device_id, device.current_state
            )
        },
        str::to_owned,
    );
    device.last_action_summary = Some(msg.clone());
    Ok(msg)
}

/// 向外部 `FHBP/1.0` C99/嵌入式固件节点发送 `POST <endpoint>/fhbp/v1/g0_halt` 硬件急刹指令（最佳努力，1500ms 超时）。
pub fn fhbp_send_g0_halt(endpoint: &str, device_id: &str, reason: &str) -> bool {
    let payload = json!({
        "protocol": FHBP_PROTOCOL_VERSION,
        "device_id": device_id,
        "reason": reason,
    });
    fhbp_http_call(endpoint, "POST", "/fhbp/v1/g0_halt", Some(&payload)).is_ok()
}

fn parse_str_vec_field(val: &Value, key: &str) -> Vec<String> {
    val.get(key)
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn parse_envelope_map(raw: Option<&Value>) -> BTreeMap<String, EnvelopeBound> {
    let mut out = BTreeMap::new();
    let Some(obj) = raw.and_then(Value::as_object) else {
        return out;
    };
    for (k, v) in obj {
        if let Some(b_obj) = v.as_object() {
            if let (Some(min), Some(max)) = (
                b_obj.get("min").and_then(Value::as_f64),
                b_obj.get("max").and_then(Value::as_f64),
            ) {
                let unit = b_obj
                    .get("unit")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                out.insert(k.clone(), EnvelopeBound { min, max, unit });
            }
        } else if let Some(arr) = v.as_array() {
            if arr.len() >= 2 {
                if let (Some(min), Some(max)) = (arr[0].as_f64(), arr[1].as_f64()) {
                    out.insert(
                        k.clone(),
                        EnvelopeBound {
                            min,
                            max,
                            unit: String::new(),
                        },
                    );
                }
            }
        }
    }
    out
}

#[allow(clippy::too_many_lines)]
fn fhbp_http_call(
    base_endpoint: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Value, CliError> {
    let without_scheme = base_endpoint
        .trim()
        .trim_end_matches('/')
        .strip_prefix("http://")
        .ok_or_else(|| CliError::Usage("FHBP endpoint must start with http://".to_owned()))?;
    let host_port = if without_scheme.contains(':') {
        without_scheme.to_owned()
    } else {
        format!("{without_scheme}:80")
    };
    let addrs: Vec<SocketAddr> = host_port
        .to_socket_addrs()
        .map_err(|err| CliError::Io {
            path: host_port.clone(),
            detail: format!("failed to resolve FHBP hardware address: {err}"),
        })?
        .collect();
    let addr = addrs.first().ok_or_else(|| CliError::Io {
        path: host_port.clone(),
        detail: "no socket address resolved for FHBP hardware bridge".to_owned(),
    })?;

    let timeout = Duration::from_millis(1500);
    let mut stream = TcpStream::connect_timeout(addr, timeout).map_err(|err| CliError::Io {
        path: format!("{base_endpoint}{path}"),
        detail: format!("FHBP hardware bridge unreachable: {err}"),
    })?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let payload = body.map_or_else(Vec::new, |v| serde_json::to_vec(v).unwrap_or_default());
    let req_head = if payload.is_empty() {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nAccept: application/json\r\nConnection: close\r\n\r\n"
        )
    } else {
        format!(
            "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        )
    };
    stream
        .write_all(req_head.as_bytes())
        .and_then(|()| {
            if payload.is_empty() {
                Ok(())
            } else {
                stream.write_all(&payload)
            }
        })
        .and_then(|()| stream.flush())
        .map_err(|err| CliError::Io {
            path: format!("{base_endpoint}{path}"),
            detail: format!("FHBP write failed: {err}"),
        })?;

    let mut reader = BufReader::new(&stream);
    let mut status_line = String::new();
    if reader
        .read_line(&mut status_line)
        .map_err(|err| CliError::Io {
            path: format!("{base_endpoint}{path}"),
            detail: err.to_string(),
        })?
        == 0
    {
        return Err(CliError::Io {
            path: format!("{base_endpoint}{path}"),
            detail: "empty HTTP response from FHBP hardware bridge".to_owned(),
        });
    }
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(500);

    let mut content_len: Option<usize> = None;
    loop {
        let mut h = String::new();
        let n = reader.read_line(&mut h).unwrap_or(0);
        if n == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_len = v.trim().parse::<usize>().ok();
            }
        }
    }

    let mut resp_bytes = Vec::new();
    if let Some(len) = content_len {
        if len > 4 * 1024 * 1024 {
            return Err(CliError::Usage(
                "FHBP response exceeded 4 MiB safety limit".to_owned(),
            ));
        }
        resp_bytes.resize(len, 0);
        reader
            .read_exact(&mut resp_bytes)
            .map_err(|err| CliError::Io {
                path: format!("{base_endpoint}{path}"),
                detail: err.to_string(),
            })?;
    } else {
        let _ = reader.read_to_end(&mut resp_bytes);
    }

    let parsed: Value = if resp_bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&resp_bytes).map_err(|err| CliError::Io {
            path: format!("{base_endpoint}{path}"),
            detail: format!("invalid JSON from FHBP bridge: {err}"),
        })?
    };

    if status_code >= 400 {
        let detail = parsed
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("FHBP bridge returned error");
        return Err(CliError::Usage(format!(
            "FHBP bridge HTTP {status_code}: {detail}"
        )));
    }
    Ok(parsed)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::{
        apply_local_twin_actuation, default_reference_hardware_devices, normalize_fhbp_endpoint,
        transition_device_to_g0_safe_state, validate_physical_envelope,
        PHYSICAL_ENVELOPE_EXCEEDED_CODE,
    };
    use serde_json::json;

    #[test]
    fn seeds_all_five_reference_hardware_terminals_and_enforces_envelopes() {
        let mut devs = default_reference_hardware_devices();
        assert_eq!(devs.len(), 5);
        assert!(devs.contains_key("drone_px4_01"));
        assert!(devs.contains_key("rccar_ackermann_01"));
        assert!(devs.contains_key("robot_arm_6dof_01"));
        assert!(devs.contains_key("cnc_mill_5axis_01"));
        assert!(devs.contains_key("camera_jib_pro_01"));

        let drone = devs.get_mut("drone_px4_01").unwrap();
        assert!(validate_physical_envelope(drone, "takeoff", &json!({"altitude_m": 35.0})).is_ok());
        let err = validate_physical_envelope(drone, "takeoff", &json!({"altitude_m": 300.0}))
            .expect_err("300m must exceed 120m ceiling");
        assert!(err.contains(PHYSICAL_ENVELOPE_EXCEEDED_CODE));

        let _ = apply_local_twin_actuation(
            drone,
            "goto_waypoint",
            &json!({"altitude_m": 40.0, "speed_mps": 8.5}),
        );
        assert_eq!(drone.current_state, "ACTUATING");
        transition_device_to_g0_safe_state(drone, "unit test brake");
        assert_eq!(drone.current_state, "LOITER_HOVER_RTL");
        assert_eq!(drone.telemetry["speed_mps"], json!(0.0));

        let cnc = devs.get("cnc_mill_5axis_01").unwrap();
        assert_eq!(cnc.requires_cap_approval_for, vec!["start_spindle"]);
        let rpm_err =
            validate_physical_envelope(cnc, "start_spindle", &json!({"spindle_rpm": 25000}))
                .expect_err("25000 RPM must fail envelope");
        assert!(rpm_err.contains(PHYSICAL_ENVELOPE_EXCEEDED_CODE));
    }

    #[test]
    fn validates_local_and_lan_fhbp_endpoints() {
        assert_eq!(
            normalize_fhbp_endpoint("http://127.0.0.1:9100/").unwrap(),
            "http://127.0.0.1:9100"
        );
        assert_eq!(
            normalize_fhbp_endpoint("http://192.168.1.42:8080").unwrap(),
            "http://192.168.1.42:8080"
        );
        assert!(normalize_fhbp_endpoint("http://8.8.8.8:9100").is_err());
        assert!(normalize_fhbp_endpoint("https://api.example.com").is_err());
    }
}
