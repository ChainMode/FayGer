# Chapter 7. Security Model

Fayger's security model has two threads:

- **Trusted origin.** A BuF artifact may itself be signed; origin and integrity are verified at load time.
- **Controlled execution.** A BuF_Instance can use only explicitly granted capabilities at runtime; undeclared capabilities are entirely invisible.

The two threads land in the loader layer and the adapter layer respectively, reinforced by the error model in Chapter 6 and the failure isolation in Chapter 4.

## 7.1 Trusted origin: signatures and digests

### Digests

Each section declares a `digest` in BuF_Manifest. The `Verify Digest` phase recomputes and compares per section:

- Any mismatch returns `LDR_DIGEST_MISMATCH`.
- The error context contains the failing `section_id`.

The Trailer's `manifest_digest` simultaneously verifies that the Manifest itself has not been truncated or modified.

### Signature scope

The byte range covered by the signature is:

```
Header || Manifest_minus_signature || Section_Index
```

Design intent:

- Even if an attacker modifies any field of the Manifest (other than the signature block) after signing, the signature is invalidated.
- Adjusting section offset / length / digest also invalidates the signature, because Section_Index is in the signed scope.
- Section bodies are not directly in the signed scope, but each section's `digest` is in the Manifest, so they are indirectly covered.

This "sign manifest, chain digests to sections" approach matches the OCI Image trade-off: the signed input has bounded size and predictable verification time.

### Supported signature algorithms (first phase)

- `ed25519`
- `ecdsa-p256`
- `rsa-pss-sha256`

The algorithm identifier and public-key reference are explicitly declared in the `signature` block of the Manifest, avoiding ASN.1 / X.509-based implicit negotiation.

### Signature verification decision table

| Condition | Result |
| --- | --- |
| enforce_signature on ∧ no signature | `Err(LDR_SIGNATURE_FAIL)`, reason `MissingSignature` |
| signature present ∧ verification fails | `Err(LDR_SIGNATURE_FAIL)`, reason `InvalidSignature` |
| signature present ∧ verification passes | `Ok` |
| enforce_signature off ∧ no signature | `Ok` (developer convenience) |

## 7.2 Enforce-signature mode

Enforce-signature mode is a load-policy switch controlled by `LoaderPolicy.require_signature`:

```idl
struct LoaderPolicy {
  require_signature: bool
  trusted_roots: TrustedRootSet
  allowed_schema_versions: VersionRange
}
```

Behavior:

- When on: any BuF must carry a signature and the signature must verify; otherwise load is rejected.
- When off: a BuF without a signature still loads (developer convenience); a BuF with an invalid signature is still rejected ("pretending to sign" is not allowed).

For release contexts (production, trusted distribution channels) we recommend defaulting `require_signature` to `true` at the configuration layer.

## 7.3 Trusted root management

```idl
interface TrustStore {
  current_roots() -> TrustedRootSet
  update(roots: TrustedRootSet)
  enforce_signature() -> bool
}
```

Update visibility constraint:

- For an interleaved sequence of `Update(roots)` and `Load(buf)` operations, every `Load(buf)` must use the trusted-root set equal to the roots set by the most recent prior `Update`.
- The constraint makes "switching roots midway through a load" semantically impossible.

Implementation strategy:

- Snapshot `TrustStore` at the entry of each `LoaderPipeline.load`; the entire load uses the snapshot.
- Explicitly forbid sharing a mutable trusted-root set during load.

## 7.4 Controlled execution: capability security model

The adapter layer uses capability-based security (matching WASI). BuF declares capability requests in the Manifest; Platform_Adapter and host policy jointly decide what is granted.

Formal:

```
granted = requested ∩ available ∩ host_policy
denied  = requested \ granted
```

### Start gate

If `manifest.required_capabilities \ granted ≠ ∅`:

- `start()` must return an error.
- `context.missing` equals the difference.
- The instance does not enter Running.

This is the hard rule of "don't start if capabilities are insufficient".

### Default deny

- Undeclared capabilities are invisible to BuF.
- A Platform_Adapter must not provide undeclared capabilities just because "the bits look right".
- Unrecognized Universal_Instruction categories return `ADP_UNSUPPORTED_INSTRUCTION`; no "silent pass-through".

## 7.5 Capability classification and trimming notes (per host)

| Host class | Typical available | Typical unavailable | Notes |
| --- | --- | --- | --- |
| Desktop | `io.*`, `net.*`, `ui.*`, `time`, `random`, `crypto`, `proc.*`, `host.*` | platform / permission dependent | near-full set |
| Server | `io.*`, `net.*`, `time`, `random`, `crypto`, `proc.*` | `ui.*` all disabled | no GUI by default |
| Browser | `net.fetch`, `net.websocket`, `ui.dom`, `time`, `random`, subset of `crypto` | `proc.*`, most `io.*` | strong trimming |
| In-App | explicitly injected by host | all denied by default | strictest; host has the final say |

## 7.6 Resource isolation as a security barrier

Resource isolation is not only a stability requirement but also part of the security barrier:

- A single BuF_Instance failure does not propagate to other instances (failure isolation property in Chapter 4).
- Quota exceedance triggers suspension, preventing one instance from exhausting host resources.
- RuntimeDataAreas across instances are mutually invisible, preventing side-channel info leakage at the data layer.

## 7.7 Security / error interaction

Signature- and capability-related failures are returned through the unified error model:

- Signature failure: `LDR_SIGNATURE_FAIL`, with `MissingSignature` / `InvalidSignature`.
- Capability denied: `ADP_CAPABILITY_DENIED`, with the denied set.
- Start-gate failure: reuse `ADP_CAPABILITY_DENIED`; `context.missing` carries the difference.

Error chains follow Chapter 6: the lower-level error goes into `cause`, the upper-level error tags its own `source_layer`, and callers can walk the chain to localize.

## 7.8 Audit recommendations

Audit itself is not within Fayger's scope, but Fayger's event bus, Lifecycle events, and Loader events already give external auditing systems sufficient anchors:

- Per-phase success / failure events on the loading chain serve as origin-verification audit points.
- Lifecycle events record each instance's start / suspend / terminate timeline.
- Capability-trimming results serve as security-decision audit points.
- Signature failure, capability denial, and quota exceedance events can be subscribed by external SIEM / audit pipelines.
