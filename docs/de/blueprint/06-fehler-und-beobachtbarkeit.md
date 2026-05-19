# Kapitel 6. Fehler und Beobachtbarkeit

Fehlermodell und Beobachtbarkeit sind zwei querschnittliche Leitungen über die drei Schichten von Fayger. Sie ermöglichen externen Aufrufern, Fehler mit derselben Semantik zu verstehen, zu lokalisieren und Ausführungen zu überwachen.

## 6.1 Einheitliches Fehlermodell

```idl
struct FaygerError {
  code: ErrorCode
  source_layer: Layer       // loader | runtime | adapter | host
  message: String
  context: Map<String, Value>
  cause: Option<Box<FaygerError>>
}
```

Feldsemantik:

- **code.** Stabiler, dokumentierbarer Bezeichner. Für Verzweigungen beim Aufrufer; Semantik ändert sich versionsübergreifend nicht.
- **source_layer.** Markiert die Schicht, die den Fehler ursprünglich erzeugt hat. Erleichtert das Routing zur richtigen Diagnose.
- **message.** Menschenlesbare Beschreibung. Kurz, dupliziert keine Code-Information.
- **context.** Werkzeugorientierte strukturierte Schlüssel-Wert-Paare. Mindestens die Schlüsselfelder, die den Fehler ausgelöst haben (offset, missing_fields, current_state, expected_range, …).
- **cause.** Schichtenübergreifende Ursachenkette. Beim Einwickeln eines Fehlers landet der untere Fehler in `cause`, um keine Information zu verschlucken.

## 6.2 Fehlererzeugung und Propagationsregeln

1. **Nahe der Quelle erzeugen.** Jede Schicht baut `FaygerError` innerhalb ihrer Grenze und markiert `source_layer`.
2. **Schicht für Schicht einwickeln.** Wenn eine obere Schicht Semantik hinzufügt, muss sie den unteren Fehler in `cause` bewahren und `source_layer` nicht überschreiben.
3. **Kontext nicht verlieren.** `context` enthält die auslösenden Schlüsselfelder.
4. **Fehler bewahren.** Eine BuF_Instance, die in `Failed` geht, muss die letzte Universal_Instruction und den entsprechenden Fehler in `failure_info` halten, bis die Instanz explizit zerstört wird.

## 6.3 Beispiel einer Fehlerkette

Eine typische dreischichtige Propagation:

```
[oben] RT_INSTANCE_FAILED      (source_layer = runtime)
         └── cause:
[mitte]  ADP_HOST_CALL_FAILED  (source_layer = adapter)
           └── cause:
[unten]    <nativer Hostfehler> (source_layer = host)
```

Das Durchlaufen von `cause` ergibt die Schichtfolge `[runtime, adapter, host]`, passend zur „von innen nach außen"-Reihenfolge der tatsächlichen Entstehung. Diese Eigenschaft wird per PBT verifiziert.

## 6.4 Übersicht der wichtigen Fehlercodes

| Quelle | Fehlercode | Auslöser |
| --- | --- | --- |
| Loader | `LDR_PARSE_FAIL` | BuF-Bytestrom ungültig |
| Loader | `LDR_DIGEST_MISMATCH` | Section-Digest-Mismatch (Eager) |
| Loader | `LDR_LAZY_DIGEST_MISMATCH` | Section-Digest-Mismatch beim ersten Lazy-Zugriff |
| Loader | `LDR_SIGNATURE_FAIL` | Signatur fehlt im Erzwingungsmodus / ungültige Signatur |
| Loader | `LDR_SCHEMA_UNSUPPORTED` | schema-Version außerhalb des unterstützten Bereichs |
| Loader | `LDR_RUNTIME_VERSION_TOO_HIGH` | Runtime_Interface-Min höher als bereitgestellt |
| Loader | `LDR_MISSING_REQUIRED_FIELD` | Manifest fehlen Pflichtfelder beim Serialisieren |
| Loader | `LDR_PROFILE_REQUIRED_SECTION_MISSING` | Erforderliche Section im aktuellen Profil nicht ladbar |
| Loader | `LDR_LAZY_SOURCE_UNAVAILABLE` | BuF_Source im nachfolgenden Lazy-Zugriff nicht verfügbar |
| Loader | `LDR_SOURCE_READ_FAILED` | BuF_Source-Lesefehler (Eager oder Lazy) |
| Runtime | `RT_ILLEGAL_TRANSITION` | Illegaler Lebenszyklusübergang |
| Runtime | `RT_IMPL_NOT_FOUND` | Routing fand keine passende Implementierung |
| Runtime | `RT_IMPL_MISSING_METHODS` | Registrierte Implementierung fehlen Pflichtmethoden |
| Runtime | `RT_QUOTA_EXCEEDED` | Ressourcennutzung überschreitet Quota |
| Runtime | `RT_INSTANCE_FAILED` | Instanz ist in Failed gegangen |
| Adapter | `ADP_NO_MATCHING_PLATFORM` | Kein Adapter passt zum aktuellen Host |
| Adapter | `ADP_UNSUPPORTED_INSTRUCTION` | Aktueller Adapter unterstützt die Anweisung nicht |
| Adapter | `ADP_CAPABILITY_DENIED` | Capabilities nach Trimming unzureichend |
| Adapter | `ADP_HOST_CALL_FAILED` | Übersetzung in Systemaufruf hostseitig gescheitert |

Loader-Fehler markieren in `context.phase` `eager` oder `lazy`, sodass Aufrufer per Phase routen können: Startfehler können Fallback auf andere Quelle / Profil oder Wiederholung auslösen; Laufzeit-Lazy-Fehler betreffen meist nur den Zugriff auf diese Section, der Aufrufer entscheidet über Degradation.

## 6.5 Beobachtbarkeits-Event-Bus

```idl
interface EventBus {
  publish(event: FaygerEvent)
  subscribe(filter: EventFilter, handler: EventHandler) -> SubscriptionId
  set_debug_enabled(enabled: bool)
}
```

Ereigniskategorien:

- **Lifecycle Event.** Zustandsübergang einer BuF_Instance, mit `from`, `to`, `timestamp`, `instance_id`.
- **Quota Event.** Ressourcenstichprobe, Überschreitung, Suspendierung.
- **Loader Event.** Phase betreten / abgeschlossen / fehlgeschlagen, mit Phasenname.
- **Adapter Event.** Anweisungsversand, Trimming-Ergebnis, Eintrag „nicht unterstützte Anweisung".
- **Debug Event.** Nur bei `set_debug_enabled(true)`. Für tiefgehende Diagnose.

### Konsistenz der Lifecycle-Ereignisfolge

Für jede legale Übergangsfolge `[(s₀, s₁), (s₁, s₂), …]` muss die ausgegebene Ereignisfolge erfüllen:

- Gleiche Länge.
- Jedes `eᵢ.from == sᵢ₋₁`, `eᵢ.to == sᵢ`.
- `eᵢ.timestamp` ist monoton nicht fallend.

## 6.6 Integration mit Host-Log- / Trace-Kanälen

Wenn der Host Log- oder Trace-Kanäle bietet (systemd-journal, OS Logger, Browser-Console, In-App-SDK-Kanal …), bildet die Adapter-Schicht interne Fayger-Ereignisse darauf ab:

- Die Abbildung wird vom Platform_Adapter selbst beschrieben; die Runtime-Schicht kennt die Kanaldifferenzen nicht.
- Ereignisstufen (debug / info / warn / error) entsprechen den Hoststufen.
- Lifecycle und Loader Events sind info; Quota / Adapter-Fehler sind warn / error.

Hat der Host keinen Logkanal, bleiben Ereignisse im EventBus für Abonnenten.

## 6.7 Debug-Schalter

Debug-Ausgabe ist standardmäßig aus. Aktivierung:

```idl
event_bus.set_debug_enabled(true)
```

Debug-Ereignisse umfassen:

- Versanddetails jeder Universal_Instruction.
- Eingaben und Ausgaben jedes Trimmings.
- Sample-Werte des Ressourcenmonitors.

Wegen des Volumens sollten Debug-Abonnenten aktiv filtern, raten oder persistieren.

## 6.8 Statusabfrage-Schnittstellen

Fayger muss für Betrieb und Diagnose folgende Abfragen anbieten:

| Abfrage | Rückgabe |
| --- | --- |
| `state(instance_id)` | Aktueller Lifecycle_State |
| `failure_info(instance_id)` | Letzte Universal_Instruction und Fehlerobjekt, das Failed auslöste |
| `list_implementations()` | Deskriptoren der registrierten Runtime_Implementations |
| `current_adapter()` | Deskriptor des aktuell ausgewählten Platform_Adapters |
| `granted_capabilities(instance_id)` | Tatsächliche Capabilities der Instanz nach Trimming |

Diese Schnittstellen sind read-only, idempotent und in jedem Zustand aufrufbar.

## 6.9 Standardmäßige Sicherheitseinstellung

- Erzwungener Signaturmodus ist standardmäßig **aus** (Entwicklerkomfort), Release-Builds sollten in der Konfigurationsschicht **an** voreinstellen.
- Debug-Ereignisse sind standardmäßig **aus**; explizit aktivieren.
- Nicht deklarierte Capabilities sind für BuF **standardmäßig verweigert**.
- Unbekannte Universal_Instruction-Kategorien sind **standardmäßig verweigert**; „stilles Durchreichen" ist nicht erlaubt.
