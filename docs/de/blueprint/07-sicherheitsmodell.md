# Kapitel 7. Sicherheitsmodell

Das Sicherheitsmodell von Fayger beruht auf zwei Linien:

- **Vertrauenswürdige Herkunft.** Ein BuF-Artefakt darf signiert sein; Herkunft und Integrität werden beim Laden überprüft.
- **Kontrollierte Ausführung.** Eine BuF_Instance darf zur Laufzeit nur explizit gewährte Capabilities nutzen; nicht deklarierte Capabilities sind komplett unsichtbar.

Beide Linien landen in der Loader- bzw. Adapter-Schicht und werden durch das Fehlermodell aus Kapitel 6 und die Fehler-Isolation aus Kapitel 4 verstärkt.

## 7.1 Vertrauenswürdige Herkunft: Signaturen und Digests

### Digests

Jede Section deklariert im BuF_Manifest einen `digest`. Die Phase `Verify Digest` berechnet je Section neu und vergleicht:

- Bei Mismatch: `LDR_DIGEST_MISMATCH`.
- Der Fehlerkontext enthält die fehlerhafte `section_id`.

Der `manifest_digest` im Trailer überprüft zugleich, dass das Manifest selbst nicht abgeschnitten oder verändert wurde.

### Signaturumfang

Die signaturabgedeckten Bytes sind:

```
Header || Manifest_minus_signature || Section_Index
```

Designabsicht:

- Selbst wenn ein Angreifer nach dem Signieren irgendein Manifest-Feld (außer dem Signaturblock selbst) ändert, ist die Signatur ungültig.
- Änderungen an Section-Offset / -Länge / -Digest invalidieren die Signatur ebenfalls (Section_Index ist im Signaturumfang).
- Section-Bodies sind nicht direkt im Signaturumfang, aber jeder `digest` ist im Manifest, also indirekt abgedeckt.

Dieser Ansatz „Manifest signieren, Sections per Digest verketten" entspricht der OCI-Image-Abwägung: kontrollierte Eingabegröße, vorhersagbare Verifikationsdauer.

### Unterstützte Signaturalgorithmen (Phase 1)

- `ed25519`
- `ecdsa-p256`
- `rsa-pss-sha256`

Algorithmus-ID und Public-Key-Referenz werden im `signature`-Block des Manifests explizit deklariert; ASN.1- / X.509-basierte implizite Aushandlung wird vermieden.

### Entscheidungstabelle der Signaturprüfung

| Bedingung | Ergebnis |
| --- | --- |
| enforce_signature an ∧ keine Signatur | `Err(LDR_SIGNATURE_FAIL)`, Grund `MissingSignature` |
| Signatur vorhanden ∧ Verifikation schlägt fehl | `Err(LDR_SIGNATURE_FAIL)`, Grund `InvalidSignature` |
| Signatur vorhanden ∧ Verifikation ok | `Ok` |
| enforce_signature aus ∧ keine Signatur | `Ok` (Entwicklerkomfort) |

## 7.2 Modus „erzwungene Signatur"

Der Modus „erzwungene Signatur" ist ein Schalter der Lade-Policy, gesteuert durch `LoaderPolicy.require_signature`:

```idl
struct LoaderPolicy {
  require_signature: bool
  trusted_roots: TrustedRootSet
  allowed_schema_versions: VersionRange
}
```

Verhalten:

- An: Jedes BuF muss eine Signatur tragen und verifizieren; sonst wird das Laden abgelehnt.
- Aus: BuFs ohne Signatur werden geladen (Entwicklerkomfort); BuFs mit ungültiger Signatur werden weiterhin abgelehnt („Signatur vortäuschen" ist nicht erlaubt).

Für Releasekontexte (Produktion, vertrauenswürdige Distribution) empfehlen wir, `require_signature` auf Konfigurationsebene auf `true` voreinzustellen.

## 7.3 Verwaltung vertrauenswürdiger Wurzeln

```idl
interface TrustStore {
  current_roots() -> TrustedRootSet
  update(roots: TrustedRootSet)
  enforce_signature() -> bool
}
```

Sichtbarkeitsregel von Updates:

- Für eine Folge `Ops` (verschachtelte `Update(roots)` und `Load(buf)`) muss jeder `Load(buf)` für die Signaturprüfung exakt die Wurzeln verwenden, die die letzte vorausgehende `Update`-Operation gesetzt hat.
- Damit ist „Wurzeln mitten im Laden wechseln" semantisch unmöglich.

Implementierung:

- An jedem Eintritt in `LoaderPipeline.load` einen Snapshot des `TrustStore` erstellen; das gesamte Laden nutzt diesen Snapshot.
- Eine geteilte mutable Wurzelmenge während des Ladens explizit verbieten.

## 7.4 Kontrollierte Ausführung: Capability-Sicherheitsmodell

Die Adapter-Schicht nutzt Capability-based Security (analog zu WASI). BuF deklariert Capability-Anforderungen im Manifest; Platform_Adapter und Host-Policy entscheiden gemeinsam, was gewährt wird.

Formal:

```
granted = requested ∩ available ∩ host_policy
denied  = requested \ granted
```

### Start-Gate

Wenn `manifest.required_capabilities \ granted ≠ ∅`:

- `start()` muss einen Fehler liefern.
- `context.missing` entspricht der Differenz.
- Die Instanz geht nicht in Running.

Das ist die harte Regel „Nicht starten, wenn Capabilities fehlen".

### Default-Deny

- Nicht deklarierte Capabilities sind für BuF unsichtbar.
- Ein Platform_Adapter darf keine nicht deklarierten Capabilities bereitstellen, nur weil „die Bits passen".
- Unbekannte Universal_Instruction-Kategorien geben `ADP_UNSUPPORTED_INSTRUCTION` zurück; kein „stilles Durchreichen".

## 7.5 Capability-Klassen und Trimming-Hinweise (pro Host)

| Hostklasse | Typisch verfügbar | Typisch nicht verfügbar | Notiz |
| --- | --- | --- | --- |
| Desktop | `io.*`, `net.*`, `ui.*`, `time`, `random`, `crypto`, `proc.*`, `host.*` | plattform-/rechteabhängig | nahezu vollständig |
| Server | `io.*`, `net.*`, `time`, `random`, `crypto`, `proc.*` | `ui.*` ganz deaktiviert | standardmäßig kein GUI |
| Browser | `net.fetch`, `net.websocket`, `ui.dom`, `time`, `random`, Teilmenge von `crypto` | `proc.*`, Großteil von `io.*` | starkes Trimming |
| In-App | vom Host explizit injiziert | standardmäßig alles deny | am restriktivsten; Host hat das letzte Wort |

## 7.6 Ressourcen-Isolation als Sicherheitsbarriere

Ressourcen-Isolation ist nicht nur eine Stabilitätsanforderung, sondern auch Teil der Sicherheitsbarriere:

- Ein einzelner BuF_Instance-Fehler propagiert nicht zu anderen Instanzen (Fehler-Isolations-Eigenschaft in Kapitel 4).
- Quota-Überschreitung löst Suspend aus und verhindert, dass eine Instanz Hostressourcen erschöpft.
- RuntimeDataAreas verschiedener Instanzen sind gegenseitig unsichtbar — verhindert Seitenkanal-Lecks auf Datenebene.

## 7.7 Sicherheits- / Fehler-Interaktion

Signatur- und Capability-bezogene Fehler werden über das einheitliche Fehlermodell zurückgegeben:

- Signaturfehler: `LDR_SIGNATURE_FAIL`, mit `MissingSignature` / `InvalidSignature`.
- Capability verweigert: `ADP_CAPABILITY_DENIED`, mit der verweigerten Menge.
- Start-Gate-Fehler: `ADP_CAPABILITY_DENIED` wiederverwenden; `context.missing` enthält die Differenz.

Fehlerketten gemäß Kapitel 6: Der unterliegende Fehler kommt in `cause`, der obere Fehler markiert sein eigenes `source_layer`; Aufrufer können die Kette zur Lokalisierung verfolgen.

## 7.8 Audit-Empfehlungen

Auditing selbst ist nicht Aufgabe von Fayger, aber EventBus, Lifecycle- und Loader-Events liefern externen Auditing-Systemen ausreichend Anker:

- Erfolg-/Fehlerereignisse jeder Lade-Phase als Audit-Punkte für Herkunftsverifikation.
- Lifecycle-Events dokumentieren Start- / Suspend- / Terminate-Zeitlinien jeder Instanz.
- Trimming-Ergebnisse als Audit-Punkte für Sicherheitsentscheidungen.
- Signatur-Fehlschläge, Capability-Verweigerungen und Quota-Überschreitungen können von externen SIEM- / Audit-Pipelines abonniert werden.
