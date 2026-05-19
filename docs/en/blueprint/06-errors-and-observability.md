# Chapter 6. Errors and Observability

The error model and observability are two cross-cutting wires across Fayger's three layers. They let external callers understand failures, locate failures, and monitor execution with one set of semantics.

## 6.1 Unified error model

```idl
struct FaygerError {
  code: ErrorCode
  source_layer: Layer       // loader | runtime | adapter | host
  message: String
  context: Map<String, Value>
  cause: Option<Box<FaygerError>>
}
```

Field semantics:

- **code.** A stable, documentable identifier. Used for caller branching; semantics never change across versions.
- **source_layer.** Tags the layer that originally produced the error. Enables routing to the right module's diagnostics.
- **message.** Human-readable description. Short, does not duplicate code information.
- **context.** Tool-facing structured key-value pairs. At minimum carries the key fields that triggered the error (offset, missing_fields, current_state, expected_range, …).
- **cause.** The causal chain across layers. Wrapping errors place the lower-layer error in cause to avoid swallowing information.

## 6.2 Error production and propagation rules

1. **Construct close to the source.** Each layer constructs `FaygerError` within its own boundary and tags `source_layer`.
2. **Wrap layer by layer.** When an upper layer adds semantics, it must preserve the lower-layer error in `cause` and must not rewrite `source_layer`.
3. **Don't lose context.** `context` carries the key fields that triggered the error.
4. **Failure preserved.** A BuF_Instance entering `Failed` must persist the last Universal_Instruction and the corresponding error in `failure_info` until the instance is explicitly destroyed.

## 6.3 Error chain example

A typical three-layer propagation:

```
[top]  RT_INSTANCE_FAILED      (source_layer = runtime)
         └── cause:
[mid]    ADP_HOST_CALL_FAILED  (source_layer = adapter)
           └── cause:
[low]      <native host error> (source_layer = host)
```

Walking `cause` yields the layer sequence `[runtime, adapter, host]`, matching the inside-out order in which the errors actually arose. The property is continuously verified by property-based testing.

## 6.4 Key error code overview

| Source | Error code | Trigger |
| --- | --- | --- |
| Loader | `LDR_PARSE_FAIL` | BuF byte stream invalid |
| Loader | `LDR_DIGEST_MISMATCH` | Section digest mismatch (Eager) |
| Loader | `LDR_LAZY_DIGEST_MISMATCH` | Section digest mismatch on first Lazy access |
| Loader | `LDR_SIGNATURE_FAIL` | Signature missing in enforce mode / invalid signature |
| Loader | `LDR_SCHEMA_UNSUPPORTED` | schema version outside supported range |
| Loader | `LDR_RUNTIME_VERSION_TOO_HIGH` | Runtime_Interface min higher than provided |
| Loader | `LDR_MISSING_REQUIRED_FIELD` | Manifest missing required fields during serialization |
| Loader | `LDR_PROFILE_REQUIRED_SECTION_MISSING` | Required section unloadable under the current profile |
| Loader | `LDR_LAZY_SOURCE_UNAVAILABLE` | BuF_Source unavailable on subsequent Lazy access |
| Loader | `LDR_SOURCE_READ_FAILED` | BuF_Source read error (Eager or Lazy) |
| Runtime | `RT_ILLEGAL_TRANSITION` | Illegal lifecycle transition |
| Runtime | `RT_IMPL_NOT_FOUND` | Routing did not find a matching implementation |
| Runtime | `RT_IMPL_MISSING_METHODS` | Registered implementation missing required methods |
| Runtime | `RT_QUOTA_EXCEEDED` | Resource usage exceeds quota |
| Runtime | `RT_INSTANCE_FAILED` | Instance entered Failed |
| Adapter | `ADP_NO_MATCHING_PLATFORM` | No adapter matches the current host |
| Adapter | `ADP_UNSUPPORTED_INSTRUCTION` | The current adapter does not support the instruction |
| Adapter | `ADP_CAPABILITY_DENIED` | Capabilities insufficient after trimming |
| Adapter | `ADP_HOST_CALL_FAILED` | Translation to system call failed on host side |

Loader-layer errors tag `context.phase` as `eager` or `lazy` so callers can route by phase: startup-time failure may trigger fallback to a different source / profile or a retry; runtime-time Lazy failure usually only affects access to that section, and the caller decides whether to degrade.

## 6.5 Observability event bus

```idl
interface EventBus {
  publish(event: FaygerEvent)
  subscribe(filter: EventFilter, handler: EventHandler) -> SubscriptionId
  set_debug_enabled(enabled: bool)
}
```

Event categories:

- **Lifecycle Event.** BuF_Instance state transition, carrying `from`, `to`, `timestamp`, `instance_id`.
- **Quota Event.** Resource sample, exceed, suspend.
- **Loader Event.** Phase enter / complete / fail, carrying phase name.
- **Adapter Event.** Instruction dispatch, capability negotiation outcome, unsupported instruction record.
- **Debug Event.** Emitted only when `set_debug_enabled(true)`. For deep diagnosis.

### Lifecycle event sequence consistency

For any legal transition sequence `[(s₀, s₁), (s₁, s₂), …]`, the emitted event sequence must satisfy:

- Same length.
- Each `eᵢ.from == sᵢ₋₁`, `eᵢ.to == sᵢ`.
- `eᵢ.timestamp` is monotonically non-decreasing.

## 6.6 Integration with host log / trace channels

When the host provides log or trace channels (systemd-journal, OS Logger, browser console, In-App SDK channel, …), the adapter layer maps Fayger internal events to that channel:

- The mapping is self-described by the Platform_Adapter; the runtime layer is unaware of channel differences.
- Event levels (debug / info / warn / error) map to corresponding host levels.
- Lifecycle Event and Loader Event are info; Quota / Adapter errors are warn / error.

If the host has no log channel, events stay in the EventBus for subscribers.

## 6.7 Debug event switch

Debug-level event output is off by default. Turn it on via:

```idl
event_bus.set_debug_enabled(true)
```

Debug events include:

- Per-instruction dispatch details.
- Per-trim input and output.
- Per-sample resource readings.

Because of volume, debug subscribers should filter, rate-limit, or persist actively.

## 6.8 State query interface

Fayger must expose the following queries for ops and diagnostics:

| Query | Returns |
| --- | --- |
| `state(instance_id)` | Current Lifecycle_State |
| `failure_info(instance_id)` | Last Universal_Instruction and error object that triggered failure |
| `list_implementations()` | Registered Runtime_Implementation descriptors |
| `current_adapter()` | Currently selected Platform_Adapter descriptor |
| `granted_capabilities(instance_id)` | The instance's actual capability set after trimming |

These interfaces are read-only, idempotent, and callable in any state.

## 6.9 Default safety stance

- Enforce-signature mode is **off** by default (developer convenience), but release builds should default to **on** at the configuration layer.
- Debug events are **off** by default; explicitly enable to opt in.
- Undeclared capabilities are **denied** by default for BuF.
- Unrecognized Universal_Instruction categories are **denied** by default; "silent pass-through" is not allowed.
