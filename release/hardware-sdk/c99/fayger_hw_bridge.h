/*
 * FayGer Hardware Bridge Protocol 1.0 (FHBP/1.0) — Single-Header C99 Vendor SDK
 *
 * Zero-dependency, portable ISO C99 header for hardware vendors integrating
 * Drones, RC Cars, Robots, CNC Machines, Camera Jibs, PLCs, RTOS boards, and
 * Embedded Linux / POSIX controllers as controlled FayGer Embodiment Hosts.
 *
 * Terminology alignment (iFay Ecosystem):
 *   - Human Prime : The natural person who holds supreme G0 supervisory authority.
 *   - Host        : The physical device / MCU / controller running or bridged to FayGer.
 *
 * Define FHBP_NO_POSIX_HTTP before including this header on bare-metal MCU/RTOS
 * targets without BSD/POSIX sockets to retain pure ISO C99 envelope enforcement,
 * watchdog supervision, and in-memory HTTP/JSON frame handling.
 *
 * License: MIT
 */

#ifndef FAYGER_HW_BRIDGE_H
#define FAYGER_HW_BRIDGE_H

#if !defined(FHBP_NO_POSIX_HTTP) && !defined(_WIN32)
#ifndef _POSIX_C_SOURCE
#define _POSIX_C_SOURCE 200809L
#endif
#endif

#include <errno.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef FHBP_NO_POSIX_HTTP
#if defined(_WIN32)
/* On Windows targets, define FHBP_NO_POSIX_HTTP or adapt Winsock in vendor glue. */
#define FHBP_NO_POSIX_HTTP 1
#else
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/select.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <unistd.h>
#endif
#endif

#ifdef __cplusplus
extern "C" {
#endif

#define FHBP_PROTOCOL_VERSION "fhbp/1.0"
#define FHBP_INTEGRATION_TIER "tier_b_c99_bridge"
#define FHBP_MAX_STR_LEN      64
#define FHBP_MAX_ACTION_LEN   64
#define FHBP_MAX_REASON_LEN   128
#define FHBP_HTTP_REQ_CAP     4096
#define FHBP_HTTP_RESP_CAP    4096

/* Physical safety envelope enforced deterministically in firmware BEFORE actuation. */
typedef struct fhbp_envelope_s {
    int32_t steering_min_deg;
    int32_t steering_max_deg;
    int32_t throttle_min_pct;
    int32_t throttle_max_pct;
    int32_t pan_min_deg;
    int32_t pan_max_deg;
    int32_t tilt_min_deg;
    int32_t tilt_max_deg;
    int32_t feed_min_mm_min;
    int32_t feed_max_mm_min;
    int32_t altitude_min_cm;
    int32_t altitude_max_cm;
    int32_t generic_min;
    int32_t generic_max;
} fhbp_envelope_t;

/* Parsed actuation request payload from POST /fhbp/v1/actuate. */
typedef struct fhbp_actuate_params_s {
    char    action[FHBP_MAX_ACTION_LEN];
    bool    has_steering_deg;
    int32_t steering_deg;
    bool    has_throttle_pct;
    int32_t throttle_pct;
    bool    has_pan_deg;
    int32_t pan_deg;
    bool    has_tilt_deg;
    int32_t tilt_deg;
    bool    has_feed_mm_min;
    int32_t feed_mm_min;
    bool    has_altitude_cm;
    int32_t altitude_cm;
    bool    has_param_value;
    int32_t param_value;
} fhbp_actuate_params_t;

/* Current zero-able actuator registers maintained by the bridge node. */
typedef struct fhbp_actuator_registers_s {
    int32_t steering_deg;
    int32_t throttle_pct;
    int32_t pan_deg;
    int32_t tilt_deg;
    int32_t feed_mm_min;
    int32_t altitude_cm;
    int32_t param_value;
} fhbp_actuator_registers_t;

/* Static/declarative vendor Host configuration. */
typedef struct fhbp_device_config_s {
    const char     *device_id;
    const char     *display_name;
    const char     *category;
    const char     *safe_state_name;
    uint32_t        watchdog_timeout_ms;
    fhbp_envelope_t envelope;
} fhbp_device_config_t;

/* Vendor hardware HAL callbacks invoked by the FHBP/1.0 bridge. */
typedef struct fhbp_callbacks_s {
    /*
     * Invoked ONLY after all physical envelope limits pass.
     * Return 0 on success, non-zero if hardware driver rejected command.
     */
    int (*on_actuate)(
        void                        *user_data,
        const fhbp_actuate_params_t *params,
        char                        *err_buf,
        size_t                       err_buf_cap
    );

    /*
     * Invoked immediately on POST /fhbp/v1/g0_halt OR dead-man watchdog timeout.
     * Must cut torque / neutral steering / halt spindle / engage safe hover-land.
     */
    void (*on_g0_halt)(void *user_data, const char *reason);

    /*
     * Writes a valid JSON object (e.g. {"battery_mv":7400,"rpm":0}) into out_json.
     * Optional: if NULL, bridge emits default actuator register telemetry.
     */
    void (*on_telemetry_json)(void *user_data, char *out_json, size_t out_json_cap);
} fhbp_callbacks_t;

/* Live stateful FHBP firmware bridge node. */
typedef struct fhbp_node_s {
    fhbp_device_config_t      config;
    fhbp_callbacks_t          callbacks;
    void                     *user_data;
    char                      current_state[FHBP_MAX_STR_LEN];
    uint64_t                  last_command_ms;
    uint32_t                  actuate_count;
    uint32_t                  g0_halt_count;
    uint32_t                  envelope_reject_count;
    fhbp_actuator_registers_t regs;
} fhbp_node_t;

/* Check whether v lies within [min_val, max_val] inclusive. */
static inline bool fhbp_check_range_i32(int32_t v, int32_t min_val, int32_t max_val) {
    return (v >= min_val) && (v <= max_val);
}

/* Clamp v into [min_val, max_val] inclusive. */
static inline int32_t fhbp_clamp_i32(int32_t v, int32_t min_val, int32_t max_val) {
    if (v < min_val) {
        return min_val;
    }
    if (v > max_val) {
        return max_val;
    }
    return v;
}

static inline void fhbp_copy_str(char *dst, size_t dst_cap, const char *src) {
    size_t n;
    if (dst == NULL || dst_cap == 0U) {
        return;
    }
    if (src == NULL) {
        dst[0] = '\0';
        return;
    }
    n = strlen(src);
    if (n >= dst_cap) {
        n = dst_cap - 1U;
    }
    memcpy(dst, src, n);
    dst[n] = '\0';
}

static inline void fhbp_zero_actuators(fhbp_node_t *node) {
    if (node == NULL) {
        return;
    }
    memset(&node->regs, 0, sizeof(node->regs));
}

/* Initialize a stateful FHBP bridge node in its configured safe state. */
static inline void fhbp_node_init(
    fhbp_node_t                *node,
    const fhbp_device_config_t *config,
    const fhbp_callbacks_t     *callbacks,
    void                       *user_data,
    uint64_t                    now_ms
) {
    if (node == NULL || config == NULL) {
        return;
    }
    memset(node, 0, sizeof(*node));
    node->config = *config;
    if (callbacks != NULL) {
        node->callbacks = *callbacks;
    }
    node->user_data = user_data;
    fhbp_copy_str(
        node->current_state,
        sizeof(node->current_state),
        (config->safe_state_name != NULL && config->safe_state_name[0] != '\0')
            ? config->safe_state_name
            : "SAFE_IDLE"
    );
    node->last_command_ms = now_ms;
    fhbp_zero_actuators(node);
}

/* Trigger immediate G0 safe halt, zeroing all actuator registers. */
static inline void fhbp_trigger_g0_halt(fhbp_node_t *node, const char *reason) {
    if (node == NULL) {
        return;
    }
    fhbp_zero_actuators(node);
    fhbp_copy_str(
        node->current_state,
        sizeof(node->current_state),
        (node->config.safe_state_name != NULL && node->config.safe_state_name[0] != '\0')
            ? node->config.safe_state_name
            : "SAFE_IDLE"
    );
    node->g0_halt_count += 1U;
    if (node->callbacks.on_g0_halt != NULL) {
        node->callbacks.on_g0_halt(
            node->user_data,
            (reason != NULL) ? reason : "g0_halt"
        );
    }
}

/*
 * Dead-man watchdog supervisor:
 * If node is currently ACTUATING and (now_ms - last_command_ms) > watchdog_timeout_ms,
 * automatically zeroes actuators, invokes on_g0_halt("watchdog_timeout"), and transitions
 * current_state back to safe_state_name. Returns true if watchdog tripped.
 */
static inline bool fhbp_check_watchdog(fhbp_node_t *node, uint64_t now_ms) {
    uint64_t elapsed_ms;
    if (node == NULL || node->config.watchdog_timeout_ms == 0U) {
        return false;
    }
    if (strcmp(node->current_state, "ACTUATING") != 0) {
        return false;
    }
    if (now_ms < node->last_command_ms) {
        return false;
    }
    elapsed_ms = now_ms - node->last_command_ms;
    if (elapsed_ms > (uint64_t)node->config.watchdog_timeout_ms) {
        fhbp_trigger_g0_halt(node, "watchdog_timeout");
        return true;
    }
    return false;
}

/* Minimal JSON string-field extractor (zero heap allocations). */
static inline bool fhbp_json_get_string(
    const char *json,
    const char *key,
    char       *out_val,
    size_t      out_cap
) {
    char        pattern[FHBP_MAX_STR_LEN + 4];
    const char *p;
    size_t      i;

    if (json == NULL || key == NULL || out_val == NULL || out_cap == 0U) {
        return false;
    }
    (void)snprintf(pattern, sizeof(pattern), "\"%s\"", key);
    p = strstr(json, pattern);
    if (p == NULL) {
        return false;
    }
    p += strlen(pattern);
    while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') {
        p++;
    }
    if (*p != ':') {
        return false;
    }
    p++;
    while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') {
        p++;
    }
    if (*p != '"') {
        return false;
    }
    p++;
    i = 0U;
    while (*p != '\0' && *p != '"' && i + 1U < out_cap) {
        if (*p == '\\' && *(p + 1) != '\0') {
            p++;
        }
        out_val[i++] = *p++;
    }
    out_val[i] = '\0';
    return (*p == '"');
}

/* Minimal JSON integer-field extractor (accepts integer or .0 JSON numbers). */
static inline bool fhbp_json_get_i32(
    const char *json,
    const char *key,
    int32_t    *out_val
) {
    char        pattern[FHBP_MAX_STR_LEN + 4];
    const char *p;
    char       *endptr;
    long        parsed;

    if (json == NULL || key == NULL || out_val == NULL) {
        return false;
    }
    (void)snprintf(pattern, sizeof(pattern), "\"%s\"", key);
    p = strstr(json, pattern);
    if (p == NULL) {
        return false;
    }
    p += strlen(pattern);
    while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') {
        p++;
    }
    if (*p != ':') {
        return false;
    }
    p++;
    while (*p == ' ' || *p == '\t' || *p == '\r' || *p == '\n') {
        p++;
    }
    if ((*p < '0' || *p > '9') && *p != '-') {
        return false;
    }
    errno = 0;
    parsed = strtol(p, &endptr, 10);
    if (endptr == p || errno != 0) {
        return false;
    }
    if (*endptr == '.') {
        endptr++;
        while (*endptr == '0') {
            endptr++;
        }
        if (*endptr >= '1' && *endptr <= '9') {
            return false;
        }
    }
    if (parsed < INT32_MIN || parsed > INT32_MAX) {
        return false;
    }
    *out_val = (int32_t)parsed;
    return true;
}

/* Validate parsed actuation parameters against the physical safety envelope. */
static inline bool fhbp_validate_envelope(
    const fhbp_envelope_t       *env,
    const fhbp_actuate_params_t *params,
    char                        *err_buf,
    size_t                       err_cap
) {
    if (env == NULL || params == NULL) {
        fhbp_copy_str(err_buf, err_cap, "null_envelope_or_params");
        return false;
    }
    if (params->has_steering_deg &&
        !fhbp_check_range_i32(params->steering_deg, env->steering_min_deg, env->steering_max_deg)) {
        (void)snprintf(
            err_buf,
            err_cap,
            "steering_deg=%d outside [%d, %d]",
            (int)params->steering_deg,
            (int)env->steering_min_deg,
            (int)env->steering_max_deg
        );
        return false;
    }
    if (params->has_throttle_pct &&
        !fhbp_check_range_i32(params->throttle_pct, env->throttle_min_pct, env->throttle_max_pct)) {
        (void)snprintf(
            err_buf,
            err_cap,
            "throttle_pct=%d outside [%d, %d]",
            (int)params->throttle_pct,
            (int)env->throttle_min_pct,
            (int)env->throttle_max_pct
        );
        return false;
    }
    if (params->has_pan_deg &&
        !fhbp_check_range_i32(params->pan_deg, env->pan_min_deg, env->pan_max_deg)) {
        (void)snprintf(
            err_buf,
            err_cap,
            "pan_deg=%d outside [%d, %d]",
            (int)params->pan_deg,
            (int)env->pan_min_deg,
            (int)env->pan_max_deg
        );
        return false;
    }
    if (params->has_tilt_deg &&
        !fhbp_check_range_i32(params->tilt_deg, env->tilt_min_deg, env->tilt_max_deg)) {
        (void)snprintf(
            err_buf,
            err_cap,
            "tilt_deg=%d outside [%d, %d]",
            (int)params->tilt_deg,
            (int)env->tilt_min_deg,
            (int)env->tilt_max_deg
        );
        return false;
    }
    if (params->has_feed_mm_min &&
        !fhbp_check_range_i32(params->feed_mm_min, env->feed_min_mm_min, env->feed_max_mm_min)) {
        (void)snprintf(
            err_buf,
            err_cap,
            "feed_mm_min=%d outside [%d, %d]",
            (int)params->feed_mm_min,
            (int)env->feed_min_mm_min,
            (int)env->feed_max_mm_min
        );
        return false;
    }
    if (params->has_altitude_cm &&
        !fhbp_check_range_i32(params->altitude_cm, env->altitude_min_cm, env->altitude_max_cm)) {
        (void)snprintf(
            err_buf,
            err_cap,
            "altitude_cm=%d outside [%d, %d]",
            (int)params->altitude_cm,
            (int)env->altitude_min_cm,
            (int)env->altitude_max_cm
        );
        return false;
    }
    if (params->has_param_value &&
        !fhbp_check_range_i32(params->param_value, env->generic_min, env->generic_max)) {
        (void)snprintf(
            err_buf,
            err_cap,
            "param_value=%d outside [%d, %d]",
            (int)params->param_value,
            (int)env->generic_min,
            (int)env->generic_max
        );
        return false;
    }
    return true;
}

static inline int fhbp_write_http_json(
    char       *resp,
    size_t      resp_cap,
    int         status_code,
    const char *status_text,
    const char *json_body
) {
    size_t body_len = (json_body != NULL) ? strlen(json_body) : 0U;
    return snprintf(
        resp,
        resp_cap,
        "HTTP/1.1 %d %s\r\n"
        "Content-Type: application/json; charset=utf-8\r\n"
        "Content-Length: %lu\r\n"
        "Connection: close\r\n"
        "Cache-Control: no-store\r\n"
        "\r\n"
        "%s",
        status_code,
        status_text,
        (unsigned long)body_len,
        (json_body != NULL) ? json_body : ""
    );
}

/* Render GET /fhbp/v1/manifest JSON payload. */
static inline void fhbp_build_manifest_json(
    fhbp_node_t *node,
    char        *out_json,
    size_t       out_cap
) {
    char telemetry_buf[512];
    telemetry_buf[0] = '\0';

    if (node->callbacks.on_telemetry_json != NULL) {
        node->callbacks.on_telemetry_json(
            node->user_data,
            telemetry_buf,
            sizeof(telemetry_buf)
        );
    }
    if (telemetry_buf[0] == '\0') {
        (void)snprintf(
            telemetry_buf,
            sizeof(telemetry_buf),
            "{\"steering_deg\":%d,\"throttle_pct\":%d,\"pan_deg\":%d,"
            "\"tilt_deg\":%d,\"feed_mm_min\":%d,\"altitude_cm\":%d,"
            "\"param_value\":%d,\"actuate_count\":%u,\"g0_halt_count\":%u}",
            (int)node->regs.steering_deg,
            (int)node->regs.throttle_pct,
            (int)node->regs.pan_deg,
            (int)node->regs.tilt_deg,
            (int)node->regs.feed_mm_min,
            (int)node->regs.altitude_cm,
            (int)node->regs.param_value,
            (unsigned)node->actuate_count,
            (unsigned)node->g0_halt_count
        );
    }

    (void)snprintf(
        out_json,
        out_cap,
        "{"
        "\"protocol\":\"%s\","
        "\"device_id\":\"%s\","
        "\"display_name\":\"%s\","
        "\"category\":\"%s\","
        "\"integration_tier\":\"%s\","
        "\"safe_state_name\":\"%s\","
        "\"current_state\":\"%s\","
        "\"watchdog_timeout_ms\":%u,"
        "\"envelope\":{"
        "\"steering_deg\":{\"min\":%d,\"max\":%d,\"unit\":\"deg\"},"
        "\"throttle_pct\":{\"min\":%d,\"max\":%d,\"unit\":\"%%\"},"
        "\"pan_deg\":{\"min\":%d,\"max\":%d,\"unit\":\"deg\"},"
        "\"tilt_deg\":{\"min\":%d,\"max\":%d,\"unit\":\"deg\"},"
        "\"feed_mm_min\":{\"min\":%d,\"max\":%d,\"unit\":\"mm/min\"},"
        "\"altitude_cm\":{\"min\":%d,\"max\":%d,\"unit\":\"cm\"},"
        "\"param_value\":{\"min\":%d,\"max\":%d,\"unit\":\"raw\"}"
        "},"
        "\"telemetry\":%s"
        "}",
        FHBP_PROTOCOL_VERSION,
        (node->config.device_id != NULL) ? node->config.device_id : "unknown-device",
        (node->config.display_name != NULL) ? node->config.display_name : "FHBP Device",
        (node->config.category != NULL) ? node->config.category : "generic",
        FHBP_INTEGRATION_TIER,
        (node->config.safe_state_name != NULL) ? node->config.safe_state_name : "SAFE_IDLE",
        node->current_state,
        (unsigned)node->config.watchdog_timeout_ms,
        (int)node->config.envelope.steering_min_deg,
        (int)node->config.envelope.steering_max_deg,
        (int)node->config.envelope.throttle_min_pct,
        (int)node->config.envelope.throttle_max_pct,
        (int)node->config.envelope.pan_min_deg,
        (int)node->config.envelope.pan_max_deg,
        (int)node->config.envelope.tilt_min_deg,
        (int)node->config.envelope.tilt_max_deg,
        (int)node->config.envelope.feed_min_mm_min,
        (int)node->config.envelope.feed_max_mm_min,
        (int)node->config.envelope.altitude_min_cm,
        (int)node->config.envelope.altitude_max_cm,
        (int)node->config.envelope.generic_min,
        (int)node->config.envelope.generic_max,
        telemetry_buf
    );
}

/*
 * Process a raw HTTP/1.1 request frame in memory and write a complete HTTP/1.1
 * JSON response into `resp`. Usable on both POSIX sockets and bare-metal lwIP /
 * UART-HTTP transports.
 */
static inline int fhbp_handle_http_request(
    fhbp_node_t *node,
    const char  *req,
    char        *resp,
    size_t       resp_cap,
    uint64_t     now_ms
) {
    char        method[16];
    char        path[128];
    const char *body;
    char        json_body[2048];

    if (node == NULL || req == NULL || resp == NULL || resp_cap == 0U) {
        return 0;
    }

    (void)fhbp_check_watchdog(node, now_ms);

    method[0] = '\0';
    path[0] = '\0';
    if (sscanf(req, "%15s %127s", method, path) != 2) {
        return fhbp_write_http_json(
            resp,
            resp_cap,
            400,
            "Bad Request",
            "{\"ok\":false,\"error\":\"MALFORMED_HTTP_REQUEST\"}"
        );
    }

    body = strstr(req, "\r\n\r\n");
    if (body != NULL) {
        body += 4;
    } else {
        body = strstr(req, "\n\n");
        body = (body != NULL) ? (body + 2) : "";
    }

    /* Route 1: GET /fhbp/v1/manifest */
    if (strcmp(method, "GET") == 0 && strcmp(path, "/fhbp/v1/manifest") == 0) {
        fhbp_build_manifest_json(node, json_body, sizeof(json_body));
        return fhbp_write_http_json(resp, resp_cap, 200, "OK", json_body);
    }

    /* Route 2: POST /fhbp/v1/g0_halt */
    if (strcmp(method, "POST") == 0 && strcmp(path, "/fhbp/v1/g0_halt") == 0) {
        char reason[FHBP_MAX_REASON_LEN];
        if (!fhbp_json_get_string(body, "reason", reason, sizeof(reason))) {
            fhbp_copy_str(reason, sizeof(reason), "remote_g0_emergency_brake");
        }
        fhbp_trigger_g0_halt(node, reason);
        (void)snprintf(
            json_body,
            sizeof(json_body),
            "{\"ok\":true,\"device_id\":\"%s\",\"current_state\":\"%s\","
            "\"safe_halt_applied\":true,\"reason\":\"%s\"}",
            (node->config.device_id != NULL) ? node->config.device_id : "unknown-device",
            node->current_state,
            reason
        );
        return fhbp_write_http_json(resp, resp_cap, 200, "OK", json_body);
    }

    /* Route 3: POST /fhbp/v1/actuate */
    if (strcmp(method, "POST") == 0 && strcmp(path, "/fhbp/v1/actuate") == 0) {
        fhbp_actuate_params_t params;
        char                  err_detail[FHBP_MAX_REASON_LEN];
        int                   cb_rc;

        memset(&params, 0, sizeof(params));
        err_detail[0] = '\0';

        if (!fhbp_json_get_string(body, "action", params.action, sizeof(params.action)) ||
            params.action[0] == '\0') {
            return fhbp_write_http_json(
                resp,
                resp_cap,
                400,
                "Bad Request",
                "{\"ok\":false,\"error\":\"FIRMWARE_ENVELOPE_REJECTED: missing action field\"}"
            );
        }

        params.has_steering_deg = fhbp_json_get_i32(body, "steering_deg", &params.steering_deg);
        params.has_throttle_pct = fhbp_json_get_i32(body, "throttle_pct", &params.throttle_pct);
        params.has_pan_deg      = fhbp_json_get_i32(body, "pan_deg", &params.pan_deg);
        params.has_tilt_deg     = fhbp_json_get_i32(body, "tilt_deg", &params.tilt_deg);
        params.has_feed_mm_min  = fhbp_json_get_i32(body, "feed_mm_min", &params.feed_mm_min);
        params.has_altitude_cm  = fhbp_json_get_i32(body, "altitude_cm", &params.altitude_cm);
        params.has_param_value  = fhbp_json_get_i32(body, "param_value", &params.param_value);

        /* Enforce physical safety envelope BEFORE invoking vendor HAL callback! */
        if (!fhbp_validate_envelope(&node->config.envelope, &params, err_detail, sizeof(err_detail))) {
            node->envelope_reject_count += 1U;
            (void)snprintf(
                json_body,
                sizeof(json_body),
                "{\"ok\":false,\"device_id\":\"%s\",\"current_state\":\"%s\","
                "\"error\":\"FIRMWARE_ENVELOPE_REJECTED: %s\"}",
                (node->config.device_id != NULL) ? node->config.device_id : "unknown-device",
                node->current_state,
                err_detail
            );
            return fhbp_write_http_json(resp, resp_cap, 400, "Bad Request", json_body);
        }

        cb_rc = 0;
        if (node->callbacks.on_actuate != NULL) {
            cb_rc = node->callbacks.on_actuate(
                node->user_data,
                &params,
                err_detail,
                sizeof(err_detail)
            );
        }
        if (cb_rc != 0) {
            node->envelope_reject_count += 1U;
            (void)snprintf(
                json_body,
                sizeof(json_body),
                "{\"ok\":false,\"device_id\":\"%s\",\"current_state\":\"%s\","
                "\"error\":\"FIRMWARE_ENVELOPE_REJECTED: %s\"}",
                (node->config.device_id != NULL) ? node->config.device_id : "unknown-device",
                node->current_state,
                (err_detail[0] != '\0') ? err_detail : "driver_rejected"
            );
            return fhbp_write_http_json(resp, resp_cap, 400, "Bad Request", json_body);
        }

        if (params.has_steering_deg) {
            node->regs.steering_deg = params.steering_deg;
        }
        if (params.has_throttle_pct) {
            node->regs.throttle_pct = params.throttle_pct;
        }
        if (params.has_pan_deg) {
            node->regs.pan_deg = params.pan_deg;
        }
        if (params.has_tilt_deg) {
            node->regs.tilt_deg = params.tilt_deg;
        }
        if (params.has_feed_mm_min) {
            node->regs.feed_mm_min = params.feed_mm_min;
        }
        if (params.has_altitude_cm) {
            node->regs.altitude_cm = params.altitude_cm;
        }
        if (params.has_param_value) {
            node->regs.param_value = params.param_value;
        }

        fhbp_copy_str(node->current_state, sizeof(node->current_state), "ACTUATING");
        node->last_command_ms = now_ms;
        node->actuate_count += 1U;

        (void)snprintf(
            json_body,
            sizeof(json_body),
            "{\"ok\":true,\"device_id\":\"%s\",\"current_state\":\"ACTUATING\","
            "\"action\":\"%s\",\"steering_deg\":%d,\"throttle_pct\":%d,"
            "\"telemetry\":{\"steering_deg\":%d,\"throttle_pct\":%d,\"pan_deg\":%d,"
            "\"tilt_deg\":%d,\"feed_mm_min\":%d,\"altitude_cm\":%d}}",
            (node->config.device_id != NULL) ? node->config.device_id : "unknown-device",
            params.action,
            (int)node->regs.steering_deg,
            (int)node->regs.throttle_pct,
            (int)node->regs.steering_deg,
            (int)node->regs.throttle_pct,
            (int)node->regs.pan_deg,
            (int)node->regs.tilt_deg,
            (int)node->regs.feed_mm_min,
            (int)node->regs.altitude_cm
        );
        return fhbp_write_http_json(resp, resp_cap, 200, "OK", json_body);
    }

    return fhbp_write_http_json(
        resp,
        resp_cap,
        404,
        "Not Found",
        "{\"ok\":false,\"error\":\"UNKNOWN_FHBP_ENDPOINT\"}"
    );
}

#ifndef FHBP_NO_POSIX_HTTP

static inline uint64_t fhbp_posix_now_ms(void) {
    struct timeval tv;
    if (gettimeofday(&tv, NULL) != 0) {
        return 0ULL;
    }
    return ((uint64_t)tv.tv_sec * 1000ULL) + ((uint64_t)tv.tv_usec / 1000ULL);
}

/*
 * Lightweight POSIX loopback/LAN HTTP/1.1 server loop with periodic dead-man
 * watchdog ticks (100 ms select() quantum).
 * Pass max_requests == 0 to serve indefinitely, or > 0 for bounded test runs.
 */
static inline int fhbp_serve_http(
    fhbp_node_t *node,
    const char  *bind_ip,
    uint16_t     port,
    uint32_t     max_requests
) {
    int                srv_fd;
    int                opt_val = 1;
    struct sockaddr_in addr;
    uint32_t           handled = 0U;

    if (node == NULL) {
        return -1;
    }

    srv_fd = socket(AF_INET, SOCK_STREAM, 0);
    if (srv_fd < 0) {
        return -1;
    }

    (void)setsockopt(srv_fd, SOL_SOCKET, SO_REUSEADDR, &opt_val, (socklen_t)sizeof(opt_val));

    memset(&addr, 0, sizeof(addr));
    addr.sin_family = AF_INET;
    addr.sin_port = htons(port);
    if (bind_ip == NULL || bind_ip[0] == '\0' || strcmp(bind_ip, "0.0.0.0") == 0) {
        addr.sin_addr.s_addr = htonl(INADDR_ANY);
    } else if (inet_pton(AF_INET, bind_ip, &addr.sin_addr) != 1) {
        (void)close(srv_fd);
        return -1;
    }

    if (bind(srv_fd, (struct sockaddr *)&addr, (socklen_t)sizeof(addr)) < 0) {
        (void)close(srv_fd);
        return -1;
    }
    if (listen(srv_fd, 16) < 0) {
        (void)close(srv_fd);
        return -1;
    }

    for (;;) {
        fd_set         rfds;
        struct timeval tv;
        int            sel_rc;

        FD_ZERO(&rfds);
        FD_SET(srv_fd, &rfds);
        tv.tv_sec = 0;
        tv.tv_usec = 100000; /* 100 ms watchdog tick */

        sel_rc = select(srv_fd + 1, &rfds, NULL, NULL, &tv);
        (void)fhbp_check_watchdog(node, fhbp_posix_now_ms());

        if (sel_rc < 0) {
            if (errno == EINTR) {
                continue;
            }
            break;
        }
        if (sel_rc == 0) {
            continue;
        }

        if (FD_ISSET(srv_fd, &rfds)) {
            struct sockaddr_in cli_addr;
            socklen_t          cli_len = (socklen_t)sizeof(cli_addr);
            int                cli_fd = accept(srv_fd, (struct sockaddr *)&cli_addr, &cli_len);
            if (cli_fd >= 0) {
                char    req_buf[FHBP_HTTP_REQ_CAP];
                char    resp_buf[FHBP_HTTP_RESP_CAP];
                ssize_t n_read = recv(cli_fd, req_buf, sizeof(req_buf) - 1U, 0);
                if (n_read > 0) {
                    int resp_len;
                    req_buf[n_read] = '\0';
                    resp_len = fhbp_handle_http_request(
                        node,
                        req_buf,
                        resp_buf,
                        sizeof(resp_buf),
                        fhbp_posix_now_ms()
                    );
                    if (resp_len > 0) {
                        size_t to_send = (size_t)resp_len;
                        if (to_send >= sizeof(resp_buf)) {
                            to_send = sizeof(resp_buf) - 1U;
                        }
                        (void)send(cli_fd, resp_buf, to_send, 0);
                    }
                }
                (void)close(cli_fd);
                handled += 1U;
                if (max_requests > 0U && handled >= max_requests) {
                    break;
                }
            }
        }
    }

    (void)close(srv_fd);
    return 0;
}

#endif /* FHBP_NO_POSIX_HTTP */

#ifdef __cplusplus
}
#endif

#endif /* FAYGER_HW_BRIDGE_H */
