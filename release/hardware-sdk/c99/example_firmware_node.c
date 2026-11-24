/*
 * FayGer Hardware Bridge Protocol 1.0 (FHBP/1.0) — Reference Multi-Profile Firmware Node
 *
 * Profiles:
 *   --profile rccar | cnc | jib | drone | robot
 *   --port <port>       (default: 17979)
 *   --bind <ipv4>       (default: 127.0.0.1)
 *   --self-test         (runs deterministic in-memory conformance checks and exits 0)
 *
 * Build & self-test (zero warnings under strict C99):
 *   cc -std=c99 -O2 -Wall -Wextra -Werror release/hardware-sdk/c99/example_firmware_node.c \
 *      -o /tmp/fayger_c99_fw_test && /tmp/fayger_c99_fw_test --self-test
 */

#include "fayger_hw_bridge.h"

typedef struct vendor_hw_state_s {
    const char *profile_name;
    uint32_t    hal_actuate_calls;
    uint32_t    hal_halt_calls;
    char        last_action[FHBP_MAX_ACTION_LEN];
    char        last_halt_reason[FHBP_MAX_REASON_LEN];
    int32_t     applied_steering_deg;
    int32_t     applied_throttle_pct;
    int32_t     applied_pan_deg;
    int32_t     applied_tilt_deg;
    int32_t     applied_feed_mm_min;
    int32_t     applied_altitude_cm;
    int32_t     applied_param_value;
    int32_t     battery_mv;
} vendor_hw_state_t;

static int vendor_on_actuate(
    void                        *user_data,
    const fhbp_actuate_params_t *params,
    char                        *err_buf,
    size_t                       err_buf_cap
) {
    vendor_hw_state_t *hw = (vendor_hw_state_t *)user_data;
    if (hw == NULL || params == NULL) {
        fhbp_copy_str(err_buf, err_buf_cap, "null_vendor_context");
        return -1;
    }

    hw->hal_actuate_calls += 1U;
    fhbp_copy_str(hw->last_action, sizeof(hw->last_action), params->action);

    if (params->has_steering_deg) {
        hw->applied_steering_deg = params->steering_deg;
    }
    if (params->has_throttle_pct) {
        hw->applied_throttle_pct = params->throttle_pct;
    }
    if (params->has_pan_deg) {
        hw->applied_pan_deg = params->pan_deg;
    }
    if (params->has_tilt_deg) {
        hw->applied_tilt_deg = params->tilt_deg;
    }
    if (params->has_feed_mm_min) {
        hw->applied_feed_mm_min = params->feed_mm_min;
    }
    if (params->has_altitude_cm) {
        hw->applied_altitude_cm = params->altitude_cm;
    }
    if (params->has_param_value) {
        hw->applied_param_value = params->param_value;
    }
    return 0;
}

static void vendor_on_g0_halt(void *user_data, const char *reason) {
    vendor_hw_state_t *hw = (vendor_hw_state_t *)user_data;
    if (hw == NULL) {
        return;
    }
    hw->hal_halt_calls += 1U;
    hw->applied_steering_deg = 0;
    hw->applied_throttle_pct = 0;
    hw->applied_pan_deg      = 0;
    hw->applied_tilt_deg     = 0;
    hw->applied_feed_mm_min  = 0;
    hw->applied_altitude_cm  = 0;
    hw->applied_param_value  = 0;
    fhbp_copy_str(hw->last_halt_reason, sizeof(hw->last_halt_reason), reason);
}

static void vendor_on_telemetry_json(void *user_data, char *out_json, size_t out_json_cap) {
    const vendor_hw_state_t *hw = (const vendor_hw_state_t *)user_data;
    if (hw == NULL) {
        return;
    }
    (void)snprintf(
        out_json,
        out_json_cap,
        "{\"profile\":\"%s\",\"battery_mv\":%d,\"hal_actuate_calls\":%u,"
        "\"hal_halt_calls\":%u,\"steering_deg\":%d,\"throttle_pct\":%d,"
        "\"pan_deg\":%d,\"tilt_deg\":%d,\"feed_mm_min\":%d,\"altitude_cm\":%d}",
        (hw->profile_name != NULL) ? hw->profile_name : "unknown",
        (int)hw->battery_mv,
        (unsigned)hw->hal_actuate_calls,
        (unsigned)hw->hal_halt_calls,
        (int)hw->applied_steering_deg,
        (int)hw->applied_throttle_pct,
        (int)hw->applied_pan_deg,
        (int)hw->applied_tilt_deg,
        (int)hw->applied_feed_mm_min,
        (int)hw->applied_altitude_cm
    );
}

static bool configure_profile(const char *profile, fhbp_device_config_t *cfg) {
    memset(cfg, 0, sizeof(*cfg));

    if (strcmp(profile, "rccar") == 0) {
        cfg->device_id                        = "fhbp-rccar-01";
        cfg->display_name                     = "Ackermann 1/10 RC Chassis";
        cfg->category                         = "rccar";
        cfg->safe_state_name                  = "NEUTRAL_BRAKE";
        cfg->watchdog_timeout_ms              = 500U;
        cfg->envelope.steering_min_deg        = -35;
        cfg->envelope.steering_max_deg        = 35;
        cfg->envelope.throttle_min_pct        = -30;
        cfg->envelope.throttle_max_pct        = 60;
        cfg->envelope.generic_min             = -100;
        cfg->envelope.generic_max             = 100;
        return true;
    }
    if (strcmp(profile, "cnc") == 0) {
        cfg->device_id                        = "fhbp-cnc-01";
        cfg->display_name                     = "3-Axis Desktop CNC Mill";
        cfg->category                         = "cnc";
        cfg->safe_state_name                  = "SPINDLE_OFF_FEED_HOLD";
        cfg->watchdog_timeout_ms              = 300U;
        cfg->envelope.feed_min_mm_min         = 0;
        cfg->envelope.feed_max_mm_min         = 2500;
        cfg->envelope.throttle_min_pct        = 0;
        cfg->envelope.throttle_max_pct        = 80;
        cfg->envelope.generic_min             = -500;
        cfg->envelope.generic_max             = 500;
        return true;
    }
    if (strcmp(profile, "jib") == 0) {
        cfg->device_id                        = "fhbp-jib-01";
        cfg->display_name                     = "Motorized Cinema Camera Jib";
        cfg->category                         = "jib";
        cfg->safe_state_name                  = "PAN_TILT_LOCKED";
        cfg->watchdog_timeout_ms              = 600U;
        cfg->envelope.pan_min_deg             = -170;
        cfg->envelope.pan_max_deg             = 170;
        cfg->envelope.tilt_min_deg            = -45;
        cfg->envelope.tilt_max_deg            = 85;
        cfg->envelope.throttle_min_pct        = -50;
        cfg->envelope.throttle_max_pct        = 50;
        cfg->envelope.generic_min             = -100;
        cfg->envelope.generic_max             = 100;
        return true;
    }
    if (strcmp(profile, "drone") == 0) {
        cfg->device_id                        = "fhbp-drone-01";
        cfg->display_name                     = "Quadrotor Micro-UAV Flight Bridge";
        cfg->category                         = "drone";
        cfg->safe_state_name                  = "HOVER_HOLD_OR_DISARM";
        cfg->watchdog_timeout_ms              = 400U;
        cfg->envelope.steering_min_deg        = -45;
        cfg->envelope.steering_max_deg        = 45;
        cfg->envelope.throttle_min_pct        = 0;
        cfg->envelope.throttle_max_pct        = 75;
        cfg->envelope.altitude_min_cm         = 0;
        cfg->envelope.altitude_max_cm         = 3000;
        cfg->envelope.generic_min             = -100;
        cfg->envelope.generic_max             = 100;
        return true;
    }
    if (strcmp(profile, "robot") == 0) {
        cfg->device_id                        = "fhbp-robot-01";
        cfg->display_name                     = "6-DOF Collaborative Manipulator";
        cfg->category                         = "robot";
        cfg->safe_state_name                  = "TORQUE_SAFE_STOP";
        cfg->watchdog_timeout_ms              = 250U;
        cfg->envelope.steering_min_deg        = -90;
        cfg->envelope.steering_max_deg        = 90;
        cfg->envelope.throttle_min_pct        = -40;
        cfg->envelope.throttle_max_pct        = 40;
        cfg->envelope.pan_min_deg             = -180;
        cfg->envelope.pan_max_deg             = 180;
        cfg->envelope.tilt_min_deg            = -90;
        cfg->envelope.tilt_max_deg            = 90;
        cfg->envelope.generic_min             = -1000;
        cfg->envelope.generic_max             = 1000;
        return true;
    }
    return false;
}

static int run_self_test(void) {
    static const char *const profiles[5] = { "rccar", "cnc", "jib", "drone", "robot" };
    fhbp_device_config_t     cfg;
    fhbp_callbacks_t         cbs;
    vendor_hw_state_t        hw;
    fhbp_node_t              node;
    char                     resp[FHBP_HTTP_RESP_CAP];
    size_t                   i;

    /* 1. Verify all 5 hardware profiles initialize cleanly */
    for (i = 0U; i < 5U; i++) {
        if (!configure_profile(profiles[i], &cfg)) {
            (void)fprintf(stderr, "FAIL: profile '%s' failed to configure\n", profiles[i]);
            return 1;
        }
    }

    /* 2. Exercise full FHBP/1.0 lifecycle on RC Car profile */
    (void)configure_profile("rccar", &cfg);
    memset(&cbs, 0, sizeof(cbs));
    cbs.on_actuate        = vendor_on_actuate;
    cbs.on_g0_halt        = vendor_on_g0_halt;
    cbs.on_telemetry_json = vendor_on_telemetry_json;

    memset(&hw, 0, sizeof(hw));
    hw.profile_name = "rccar";
    hw.battery_mv   = 7600;

    fhbp_node_init(&node, &cfg, &cbs, &hw, 1000ULL);

    /* 2a. GET /fhbp/v1/manifest */
    (void)fhbp_handle_http_request(
        &node,
        "GET /fhbp/v1/manifest HTTP/1.1\r\nHost: localhost\r\n\r\n",
        resp,
        sizeof(resp),
        1010ULL
    );
    if (strstr(resp, "HTTP/1.1 200 OK") == NULL ||
        strstr(resp, "\"protocol\":\"fhbp/1.0\"") == NULL ||
        strstr(resp, "\"integration_tier\":\"tier_b_c99_bridge\"") == NULL ||
        strstr(resp, "\"steering_deg\":{\"min\":-35,\"max\":35,\"unit\":\"deg\"}") == NULL ||
        strstr(resp, "\"current_state\":\"NEUTRAL_BRAKE\"") == NULL) {
        (void)fprintf(stderr, "FAIL: manifest check failed:\n%s\n", resp);
        return 1;
    }

    /* 2b. Valid actuation within physical safety envelope [-35..35 deg, -30..60 %] */
    (void)fhbp_handle_http_request(
        &node,
        "POST /fhbp/v1/actuate HTTP/1.1\r\nContent-Type: application/json\r\n\r\n"
        "{\"action\":\"drive\",\"steering_deg\":15,\"throttle_pct\":25}",
        resp,
        sizeof(resp),
        1100ULL
    );
    if (strstr(resp, "HTTP/1.1 200 OK") == NULL ||
        strstr(resp, "\"current_state\":\"ACTUATING\"") == NULL ||
        hw.hal_actuate_calls != 1U ||
        hw.applied_steering_deg != 15 ||
        hw.applied_throttle_pct != 25) {
        (void)fprintf(stderr, "FAIL: valid actuate check failed:\n%s\n", resp);
        return 1;
    }

    /* 2c. Out-of-envelope actuation MUST be rejected BEFORE vendor HAL callback */
    (void)fhbp_handle_http_request(
        &node,
        "POST /fhbp/v1/actuate HTTP/1.1\r\nContent-Type: application/json\r\n\r\n"
        "{\"action\":\"drive\",\"steering_deg\":85,\"throttle_pct\":95}",
        resp,
        sizeof(resp),
        1200ULL
    );
    if (strstr(resp, "HTTP/1.1 400 Bad Request") == NULL ||
        strstr(resp, "FIRMWARE_ENVELOPE_REJECTED:") == NULL ||
        hw.hal_actuate_calls != 1U ||
        node.envelope_reject_count != 1U) {
        (void)fprintf(stderr, "FAIL: envelope rejection check failed:\n%s\n", resp);
        return 1;
    }

    /* 2d. Dead-man watchdog timeout (> 500 ms since last valid command at 1100 ms) */
    if (!fhbp_check_watchdog(&node, 1700ULL)) {
        (void)fprintf(stderr, "FAIL: watchdog did not trip after timeout\n");
        return 1;
    }
    if (strcmp(node.current_state, "NEUTRAL_BRAKE") != 0 ||
        node.regs.steering_deg != 0 ||
        node.regs.throttle_pct != 0 ||
        hw.hal_halt_calls != 1U ||
        strcmp(hw.last_halt_reason, "watchdog_timeout") != 0) {
        (void)fprintf(stderr, "FAIL: watchdog did not restore safe state\n");
        return 1;
    }

    /* 2e. Re-actuate (using faygerd nested params + .0 numbers), then trigger Red G0 Halt */
    (void)fhbp_handle_http_request(
        &node,
        "POST /fhbp/v1/actuate HTTP/1.1\r\nContent-Type: application/json\r\n\r\n"
        "{\"protocol\":\"fhbp/1.0\",\"action\":\"drive\",\"params\":{\"steering_deg\":-10.0,\"throttle_pct\":20.0}}",
        resp,
        sizeof(resp),
        2000ULL
    );
    if (strcmp(node.current_state, "ACTUATING") != 0) {
        (void)fprintf(stderr, "FAIL: re-actuate did not enter ACTUATING\n");
        return 1;
    }

    (void)fhbp_handle_http_request(
        &node,
        "POST /fhbp/v1/g0_halt HTTP/1.1\r\nContent-Type: application/json\r\n\r\n"
        "{\"reason\":\"human_prime_red_g0_button\"}",
        resp,
        sizeof(resp),
        2050ULL
    );
    if (strstr(resp, "HTTP/1.1 200 OK") == NULL ||
        strstr(resp, "\"safe_halt_applied\":true") == NULL ||
        strcmp(node.current_state, "NEUTRAL_BRAKE") != 0 ||
        node.regs.steering_deg != 0 ||
        node.regs.throttle_pct != 0 ||
        hw.applied_steering_deg != 0 ||
        hw.applied_throttle_pct != 0 ||
        hw.hal_halt_calls != 2U) {
        (void)fprintf(stderr, "FAIL: POST /fhbp/v1/g0_halt failed:\n%s\n", resp);
        return 1;
    }

    /* 2f. Clamp helper sanity check */
    if (fhbp_clamp_i32(-999, -35, 35) != -35 || fhbp_clamp_i32(999, -35, 35) != 35) {
        (void)fprintf(stderr, "FAIL: fhbp_clamp_i32 boundary check failed\n");
        return 1;
    }

    (void)printf(
        "PASS: FHBP/1.0 C99 firmware SDK self-test succeeded "
        "(5 profiles, manifest, envelope gate, dead-man watchdog, G0 emergency halt).\n"
    );
    return 0;
}

static void print_usage(const char *prog) {
    (void)printf(
        "Usage: %s [--profile rccar|cnc|jib|drone|robot] [--bind 127.0.0.1] [--port 17979] [--self-test]\n",
        prog
    );
}

int main(int argc, char **argv) {
    const char          *profile  = "rccar";
    const char          *bind_ip  = "127.0.0.1";
    uint16_t             port     = 17979U;
    bool                 selftest = false;
    int                  i;
    fhbp_device_config_t cfg;
    fhbp_callbacks_t     cbs;
    vendor_hw_state_t    hw;
    fhbp_node_t          node;

    for (i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--self-test") == 0) {
            selftest = true;
        } else if (strcmp(argv[i], "--profile") == 0 && i + 1 < argc) {
            profile = argv[++i];
        } else if (strcmp(argv[i], "--bind") == 0 && i + 1 < argc) {
            bind_ip = argv[++i];
        } else if (strcmp(argv[i], "--port") == 0 && i + 1 < argc) {
            long p = strtol(argv[++i], NULL, 10);
            if (p <= 0L || p > 65535L) {
                (void)fprintf(stderr, "Invalid port: %s\n", argv[i]);
                return 1;
            }
            port = (uint16_t)p;
        } else if (strcmp(argv[i], "--help") == 0 || strcmp(argv[i], "-h") == 0) {
            print_usage(argv[0]);
            return 0;
        } else {
            (void)fprintf(stderr, "Unknown argument: %s\n", argv[i]);
            print_usage(argv[0]);
            return 1;
        }
    }

    if (selftest) {
        return run_self_test();
    }

    if (!configure_profile(profile, &cfg)) {
        (void)fprintf(
            stderr,
            "Unsupported profile '%s'. Choose: rccar | cnc | jib | drone | robot\n",
            profile
        );
        return 1;
    }

    memset(&cbs, 0, sizeof(cbs));
    cbs.on_actuate        = vendor_on_actuate;
    cbs.on_g0_halt        = vendor_on_g0_halt;
    cbs.on_telemetry_json = vendor_on_telemetry_json;

    memset(&hw, 0, sizeof(hw));
    hw.profile_name = profile;
    hw.battery_mv   = 12000;

#ifndef FHBP_NO_POSIX_HTTP
    fhbp_node_init(&node, &cfg, &cbs, &hw, fhbp_posix_now_ms());
    (void)printf(
        "FHBP/1.0 Firmware Node '%s' (%s) listening on http://%s:%u\n"
        "  GET  /fhbp/v1/manifest\n"
        "  POST /fhbp/v1/actuate\n"
        "  POST /fhbp/v1/g0_halt\n",
        cfg.device_id,
        cfg.display_name,
        bind_ip,
        (unsigned)port
    );
    (void)fflush(stdout);
    if (fhbp_serve_http(&node, bind_ip, port, 0U) != 0) {
        (void)fprintf(stderr, "Failed to bind FHBP HTTP server on %s:%u\n", bind_ip, (unsigned)port);
        return 1;
    }
    return 0;
#else
    (void)bind_ip;
    (void)port;
    fhbp_node_init(&node, &cfg, &cbs, &hw, 0ULL);
    (void)fprintf(stderr, "Built with FHBP_NO_POSIX_HTTP; use --self-test or custom transport.\n");
    return 1;
#endif
}
