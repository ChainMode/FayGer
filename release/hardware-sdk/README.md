# FayGer Hardware Vendor Embodiment SDK (`FHBP/1.0`) / 硬件厂商具身接入套件

This directory provides the zero-dependency integration kit for hardware manufacturers—**Drones, Ackermann RC Cars, Collaborative Robots, CNC Mills, Motorized Camera Jibs, PLCs, and RTOS/Embedded Linux controllers**—to connect physical machines safely as controlled **Host** (`宿主`) devices under **FayGer** supervision.

本目录面向无人机、阿克曼遥控车（RC Car）、协作机械臂、数控机床（CNC）、电动摄影摇臂（Camera Jib）、PLC 与嵌入式控制器厂商，提供接入 **FayGer** 受控具身运行时的三层标准化方案。

> **Terminology Boundary / 术语红线**：
> - **Human Prime（人类原型）**：持有最高 `G0` 监督与急停主权的自然人；严禁使用废弃称谓。
> - **Host（宿主）**：运行 `FayGer` 或通过 `FHBP/1.0` 桥接协议受控执行的物理设备、嵌入式板卡或终端进程。
> - **GMChain（公誉链）** & **MeriToken（美誉值，`μ`）**：记录受控履约信誉与计算优先权消耗的公誉账本与计量单位。

---

## Three Integration Tiers / 三档硬件厂商接入路径

| Tier / 档位 | Target Hardware / 适用硬件宿主 | Runtime Footprint / 集成形态 | Safety Gate / 安全门禁 |
| :--- | :--- | :--- | :--- |
| **Tier A — Embedded Linux Direct Binary**<br>嵌入式 Linux 直装引擎 | Raspberry Pi, Jetson Orin, RK3588, Industrial x86_64/ARM64 IPC, Companion Computers | Prebuilt `fayger-aarch64-unknown-linux-gnu` or `fayger-x86_64-unknown-linux-gnu` + `systemd` unit | Full `BuF ∩ HostPool ∩ PerFayPolicy` + **CAP** + **Faying** + **GMChain** / **MeriToken** (`μ`) |
| **Tier B — C99 Firmware Bridge (`FHBP/1.0`)**<br>零依赖 C99 固件桥接协议 | MCU / FreeRTOS / Zephyr / Embedded C controllers for **RC Cars, CNCs, Camera Jibs, Drones, Robots** | Single header [`c99/fayger_hw_bridge.h`](./c99/fayger_hw_bridge.h) (~500 lines ISO C99, zero third-party deps) | Firmware-level `fhbp_envelope_t` pre-check + Dead-Man Watchdog + Instant `POST /fhbp/v1/g0_halt` |
| **Tier C — Custom OS Rust `HostAdapter` HAL**<br>定制系统 Rust 5 函数硬件抽象层 | Proprietary Robot OS, Custom Hypervisor, Safety-Certified Rust Runtime | Implement the 5-function `HostAdapter` trait in [`../dev/core/`](../dev/core/README.md) and pass `fayger conformance` | Black-box `Runtime_Interface` conformance + hardware capability isolation |

---

## Tier A: Embedded Linux Companion Board (`systemd` Direct Install)

On any `aarch64` or `x86_64` Linux companion board (e.g., Raspberry Pi 5, NVIDIA Jetson, Rockchip RK3588):

```bash
curl -fsSL https://raw.githubusercontent.com/ChainMode/FayGer/main/scripts/install.sh | sh
```

Create `/etc/systemd/system/faygerd.service` so the **Host** boots into a supervised `SAFE` standby state (never auto-actuating without an explicit **Faying** mandate from the **Human Prime**):

```ini
[Unit]
Description=FayGer Autonomous Avatar Runtime Daemon (Embedded Host)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=fayger
ExecStart=/home/fayger/.fayger/bin/fayger up --bind 127.0.0.1:7979
Restart=on-failure
RestartSec=2
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
```

---

## Tier B: Single-Header C99 Firmware SDK (`FHBP/1.0`)

For real-time motor/flight/spindle controllers written in C/C++, drop [`c99/fayger_hw_bridge.h`](./c99/fayger_hw_bridge.h) into your firmware tree. See [`c99/example_firmware_node.c`](./c99/example_firmware_node.c) for a complete runnable reference supporting `rccar`, `cnc`, `jib`, `drone`, and `robot` profiles.

### 1. Compile & Run Deterministic Self-Test (`-std=c99 -Wall -Wextra -Werror`)

```bash
cc -std=c99 -O2 -Wall -Wextra -Werror \
  release/hardware-sdk/c99/example_firmware_node.c \
  -o /tmp/fayger_c99_fw_test && \
  /tmp/fayger_c99_fw_test --self-test && \
  rm -f /tmp/fayger_c99_fw_test
```

### 2. Run a Live Hardware Profile Bridge

```bash
cc -std=c99 -O2 -Wall -Wextra -Werror \
  release/hardware-sdk/c99/example_firmware_node.c \
  -o /tmp/fayger_fw_node

# Launch Ackermann RC Car bridge (or: cnc | jib | drone | robot)
/tmp/fayger_fw_node --profile rccar --bind 127.0.0.1 --port 17979
```

### 3. `FHBP/1.0` Wire Endpoints & Dual-Lock Safety Guarantees

Every `FHBP/1.0` node enforces **physical safety limits inside the firmware** independently of any upstream LLM or network host:

1. **`GET /fhbp/v1/manifest`** — Returns device identity, `"protocol": "fhbp/1.0"`, `"integration_tier": "tier_b_c99_bridge"`, `safe_state_name`, `current_state`, `watchdog_timeout_ms`, physical `envelope` bounds (`fhbp_envelope_t`), and live `telemetry`.
2. **`POST /fhbp/v1/actuate`** — Parses `"action"`, `"steering_deg"`, `"throttle_pct"`, `"pan_deg"`, `"tilt_deg"`, `"feed_mm_min"`, `"altitude_cm"`, and `"param_value"`.
   - **Pre-Callback Envelope Gate**: Calls `fhbp_validate_envelope()` (`fhbp_check_range_i32`) **before** invoking `on_actuate`. Out-of-bounds commands are rejected with HTTP `400` and `"FIRMWARE_ENVELOPE_REJECTED: ..."` without touching PWM/servo/spindle registers.
3. **`POST /fhbp/v1/g0_halt`** — Triggered when the **Human Prime** presses the red **`G0` Emergency Brake** on the Desktop Dashboard or Mobile Pocket Guardian. Immediately invokes `on_g0_halt`, zeroes all actuator registers, transitions `current_state` to `safe_state_name`, and returns `"safe_halt_applied": true`.
4. **Dead-Man Watchdog (`fhbp_check_watchdog`)** — If `watchdog_timeout_ms` elapses without a fresh valid actuation heartbeat while `ACTUATING`, the firmware autonomously triggers `on_g0_halt("watchdog_timeout")` and returns to `safe_state_name`.
5. **Bare-Metal / RTOS Portability** — Define `#define FHBP_NO_POSIX_HTTP` before `#include "fayger_hw_bridge.h"` on FreeRTOS, Zephyr, or STM32/ESP32 targets without POSIX sockets, and feed UART/CAN/lwIP buffers directly to `fhbp_handle_http_request()`.

---

## Tier C: Custom OS Rust `HostAdapter` (5-Function HAL)

Vendors building a custom Rust-based robotics OS or safety supervisor can embed `fayger-runtime` directly from [`../dev/core/`](../dev/core/README.md) by implementing the 5-method `HostAdapter` Hardware Abstraction Layer:

1. `adapter_id(&self) -> &str` — Canonical **Host** adapter identifier.
2. `advertised_capabilities(&self) -> CapabilitySet` — Static hardware capabilities exposed to `HostPool`.
3. `probe_permissions(&self) -> PermissionSnapshot` — Live physical interlock / key-switch / sensor readiness probe.
4. `execute_instruction(&mut self, inst: &UniversalInstruction) -> Result<ExecutionReceipt, AdapterError>` — **CAP**-verified actuation after `BuF ∩ HostPool ∩ PerFayPolicy` intersection passes.
5. `emergency_halt_g0(&mut self, reason: &str) -> SafeHaltReceipt` — Deterministic `G0` hardware safe-stop isolation (**Rogue Fay** containment).

After implementing `HostAdapter`, run `fayger conformance` to verify 100% black-box behavioral equivalence with the reference **FayGer** runtime.
