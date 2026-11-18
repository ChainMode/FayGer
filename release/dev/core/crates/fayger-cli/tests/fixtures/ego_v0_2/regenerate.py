#!/usr/bin/env python3
"""Regenerate the Ego v0.2 cross-language interop fixtures for FayGer.

Produces two signed multi-edition packages (editions lite/normal/plus, deep grid
L1..L3) using Ego's real `ego train` pipeline with the Stub training backend:

* ``stub-measured/``: exactly what `ego train` writes for a stub-trained package.
  ``cell_consistency_score`` is null by design (placeholder replies are not model
  behaviour), so FayGer's production policy must reject it.
* ``gate-ready/``: the same package after Ego's own ``refresh_manifest_fidelity``
  re-signs it with a fidelity report shaped like a real-backend run.
  NOTE: those fidelity numbers are SYNTHETIC (no real model was trained). This
  fixture tests the signing and policy contract, not model quality.

Usage (from anywhere):
    EGO_SRC=/path/to/ego/release/dev/src python3 regenerate.py

Signing material is a fixed public test seed. It is not a secret.
"""

from __future__ import annotations

import json
import os
import shutil
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
SEED_HEX = "5a" * 32
SIGNER_FAY_ID = "ifay_interopfixtureaaaaa"

ego_src = os.environ.get("EGO_SRC")
if not ego_src:
    sys.exit("set EGO_SRC to ego/release/dev/src")
sys.path.insert(0, ego_src)
os.environ["EGO_SIGNING_SEED_HEX"] = SEED_HEX
os.environ["EGO_SIGNER_FAY_ID"] = SIGNER_FAY_ID

import yaml  # noqa: E402

from ego_pod.cli import main  # noqa: E402
from ego_pod.trainer import mlx_backend, peft_backend, unsloth_backend  # noqa: E402
from ego_pod.vault.package import refresh_manifest_fidelity  # noqa: E402
from ego_pod.vault.signing import derive_ego_signing_subkey  # noqa: E402

# Force the Stub backend regardless of locally installed ML libraries.
for mod in (peft_backend, mlx_backend, unsloth_backend):
    for name in dir(mod):
        obj = getattr(mod, name)
        if isinstance(obj, type) and "is_available" in obj.__dict__:
            obj.is_available = classmethod(lambda cls: False)

CORPUS = {
    "sense": ["Morning desk notes, calm and focused, entry %d." % i for i in range(4)],
    "action": ["Run the local test command and read the log output."],
    "skill": [
        "Wrap configuration in an immutable dataclass so compile and train stages share no hidden state.",
        "```python\ndef verify(payload: dict, sig: bytes) -> bool:\n    # RFC 8785 JCS + HKDF-SHA256 subkey\n    return True\n```\n"
        "Because keys sort by UTF-16 code units, implementations agree byte for byte.",
    ],
    "thought": [
        "Reading note %d: a short thought about tool simplicity." % i for i in range(3)
    ]
    + [
        "Architecture note %d: constrained devices need on-demand loading to separate core personality from deep skills." % i
        for i in range(3, 6)
    ]
    + [
        "Theory note %d: prove the monotone consistency of contiguous quantile compaction under small-sample degeneration while trading rank capacity against size ceilings." % i
        for i in range(6, 8)
    ],
    "credentials": ["Never hardcode keys in the repository."],
}

with tempfile.TemporaryDirectory() as tmp:
    ws = Path(tmp)
    for category, texts in CORPUS.items():
        d = ws / "ego-data" / category
        d.mkdir(parents=True)
        for i, text in enumerate(texts):
            (d / f"{category}_{i}.txt").write_text(text, encoding="utf-8")
    cwd = os.getcwd()
    os.chdir(ws)
    try:
        assert main(["init"]) == 0
        cfg = yaml.safe_load(Path("ego-config.yaml").read_text(encoding="utf-8"))
        cfg["editions"] = {
            "lite": {"base": "qwen3.5-0.8B"},
            "normal": {"base": "qwen3.5-0.8B"},
            "plus": {"base": "qwen3.5-4B"},
        }
        cfg["training"]["epochs"] = 1
        Path("ego-config.yaml").write_text(yaml.safe_dump(cfg), encoding="utf-8")
        assert main(["train"]) == 0
    finally:
        os.chdir(cwd)

    out = ws / "ego-output"
    shutil.rmtree(out / ".history", ignore_errors=True)
    for target in ("stub-measured", "gate-ready"):
        dest = HERE / target
        shutil.rmtree(dest, ignore_errors=True)
        shutil.copytree(out, dest)

gate = HERE / "gate-ready"
report = json.loads((gate / "manifest.json").read_text(encoding="utf-8"))["fidelity_report"]
report.update(
    status="measured",
    holdout_loss=2.5,
    holdout_perplexity=12.182494,
    cell_consistency_score=0.86,
)
refresh_manifest_fidelity(gate, report)

from cryptography.hazmat.primitives import serialization  # noqa: E402
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey  # noqa: E402

pub = (
    Ed25519PrivateKey.from_private_bytes(derive_ego_signing_subkey(SEED_HEX))
    .public_key()
    .public_bytes(serialization.Encoding.Raw, serialization.PublicFormat.Raw)
    .hex()
)
(HERE / "trusted_pubkey.hex").write_text(pub + "\n", encoding="utf-8")
(HERE / "signer_fay_id.txt").write_text(SIGNER_FAY_ID + "\n", encoding="utf-8")
print("regenerated", HERE)
