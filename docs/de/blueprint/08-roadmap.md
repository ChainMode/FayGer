# Kapitel 8. Roadmap

Dieses Kapitel definiert die Grenzen der ersten Phase und skizziert die Evolution entlang bekannter Richtungen. Jeder Punkt nennt seinen Wirkungsbereich auf die heutige Architektur, damit künftige Designs konvergent bleiben.

## 8.1 Umfang der ersten Phase

Eine Erstauslieferung von Fayger erfüllt mindestens:

- Stabile Verträge für die drei Schichten (Loader / Runtime / Adapter).
- Fünfteiliges BuF-Artefaktformat mit deterministischem CBOR-Manifest / Section_Index.
- Ladekette: Read(Header/Manifest/Index) → Parse → Verify(Structural) → Verify(Digest of Header/Manifest/Index) → Verify(Signature) → Negotiate Version → Select(Sections by LoadProfile) → Resolve → Read(Selected Section Bodies)? → HandOff.
- BuF_Source-Abstraktion mit Unterstützung beliebiger Speicher-Backends (lokal, Netzwerk, Object Storage, benutzerdefiniert).
- Partielles Laden (profilgesteuerte Section-Auswahl) und Lazy Loading (Eager / Lazy LoadStrategy) für Größen- und Laufzeit-Fetch-Anforderungen eingeschränkter Endgeräte (Drohnen, Kameras).
- Interpretierter Ausführungspfad; Lebenszyklus-Zustandsautomat und Fehler-Isolation der BuF_Instance.
- 8-Kategorien-Universal_Instruction und vier eingebaute Platform_Adapter (Desktop / Server / Browser / In-App).
- Einheitliches Fehlermodell und Beobachtbarkeits-Event-Bus; Loader-Fehler tragen ein `phase`-Tag.
- Signaturprüfung, erzwungener Signaturmodus, Sichtbarkeit von Trust-Root-Updates.

## 8.2 Nicht-Ziele der ersten Phase

Zur Begrenzung der Komplexität in Phase 1 wird Folgendes ausdrücklich nicht ausgeliefert:

- Geteilter Speicher und IPC zwischen BuF_Instances.
- JIT-Kompilierung und Maschinencodeerzeugung.
- Verteilte Planung und Multi-Knoten-Fayger-Kollaboration.
- Differenzielle Verteilung von BuF und artefaktübergreifender Layered Cache (partielles Laden deckt artefaktinternes Section-Slicing ab; artefaktübergreifende Differenz-Wiederverwendung und Distribution-Protokoll werden später behandelt).
- Eingebautes Remote-Repository / Distribution-Protokoll.

Ein Nicht-Ziel ist nicht „nie nötig", sondern „heute nicht in der Vertragsfläche". Spätere Phasen dürfen vorhandene Verträge nicht brechen.

## 8.3 Phase 2: Ausführungsleistung

Richtung: effizientere Ausführungspfade hinzufügen, ohne Runtime_Interface zu brechen.

### 8.3.1 ExecutionStrategy-Abstraktion

„Wie wird BuF-Semantik ausgeführt" als Strategie extrahiert:

```idl
interface ExecutionStrategy {
  prepare(obj: BuFObject) -> PreparedExecutable
  step(exec: PreparedExecutable, ctx: ExecutionContext) -> StepResult
}
```

Phase 1 liefert `Interpreter` als eine Strategie. Spätere können koexistieren:

- `JIT`: Zwischendarstellung in der BuF-Code-Section in Maschinencode kompilieren.
- `AOT`: Maschinencode beim Laden erzeugen (geeignet für Releases).
- `HybridTier`: Interpretieren → Probing → Kompilieren.

Strategiewahl ist intern in einer Runtime_Implementation und ändert nichts am externen Vertrag des Runtime_Interface.

### 8.3.2 Performance-Beobachtbarkeit

Neue Ereigniskategorien:

- `ExecutionMetric`: Sampling von Interpretation / Kompilierung / GC.
- `TierTransition`: Wechsel zwischen Ausführungsstufen.

Konkrete Strategien werden außen nicht exponiert, ihr Effekt ist aber beobachtbar.

## 8.4 Phase 3: Distribution und Cache

### 8.4.1 Artefaktübergreifende Layer und Chunks

Phase 1 unterstützt bereits Section-granulares On-Demand-Lesen innerhalb eines BuF (Lazy). Spätere Phasen führen artefaktübergreifendes Layer-Sharing ein:

- BuF-übergreifend gemeinsam genutzte Ressourcen (Standardbibliothek, Modellgewichte) werden als unabhängige Layer extrahiert.
- Manifest erhält ein optionales `layer_refs`, das Sections in anderen Artefakten referenziert.
- Ein Distributionsprotokoll holt Layer-Ressourcen geteilt; lokale Cache-Treffer ermöglichen Wiederverwendung über BuFs hinweg.

Erfordert neue optionale Felder im Manifest und entsprechend ein Minor-Bump von `schema_version`. Bestehende BuFs sind nicht betroffen (Kompatibilitätspfad in §8.7).

### 8.4.2 Distributionsprotokoll

Einführung eines leichtgewichtigen Distributionsprotokolls (an OCI Distribution Spec angelehnt):

- Drei Schritte: `discover` / `pull` / `verify`.
- Das Protokoll liegt nicht im Fayger-Prozess; eine eigenständige Komponente implementiert es. Fayger konsumiert nur die produzierten Chunks.

### 8.4.3 Lokaler Cache

Für bereits geladene und signaturgeprüfte BuFs werden Manifest- und Section-Parseergebnisse über Prozessgrenzen gecached. Bei Treffern wird die Signaturprüfung erneut ausgeführt (gegen Cache-Vergiftung).

## 8.5 Phase 4: Instanzübergreifende Kollaboration

### 8.5.1 Geteilter Speicher (kontrolliert)

Mehrere BuF_Instances innerhalb desselben Fayger dürfen einen Speicherbereich teilen:

- Im Manifest die Capability `shared_memory` explizit deklarieren.
- Die Runtime allokiert die Bereiche zentral und legt sie gemäß Capability-Vergaben offen.
- Geteilte Bereiche stehen unter Quota-Monitoring; Verstöße lösen Suspend aus.

### 8.5.2 IPC-Protokoll

Inter-Instance-Messaging (im selben Prozess oder im selben Fayger):

- Als neue Universal_Instruction-Kategorie `ipc` ausgedrückt.
- Default-Deny; das Manifest muss Empfänger-IDs oder Topics deklarieren.

### 8.5.3 Abhängigkeitsmontage in einem einzelnen Fayger

Ein BuF darf andere BuFs als Abhängigkeiten referenzieren:

- Die `Resolve`-Phase der Ladekette unterstützt rekursive Auflösung.
- Der Abhängigkeitsgraph wird über das Manifest-Feld `dependencies` ausgedrückt.
- Zyklische Abhängigkeiten werden in Resolve abgelehnt.

## 8.6 Phase 5: verteilt

Richtung: Multi-Knoten-Fayger-Kollaboration. Nicht in Phase 1, aber Spielraum bleibt:

- BuF_Instance-IDs müssen knotenübergreifend eindeutig adressierbar sein.
- Lifecycle-Event-Zeitstempel müssen mit externen Tracing-Systemen ausrichtbar sein.
- Die TLV-Codierung von Universal_Instruction erlaubt bereits Netzwerk-Transport; „Remote-Anweisungsversand" kann eine eigene Adapter-Implementierung werden.

## 8.7 Kompatibilitätsstrategie

Jede Evolution muss diese Regeln einhalten; sonst gilt sie als Breaking Change:

1. **Runtime_Interface-Version.** Nach SemVer erhöhen, wenn Methoden hinzukommen oder sich Semantik ändert. BuF_Manifest drückt seine Mindestabhängigkeit über `runtime_interface_min` aus.
2. **BuF-schema-Version.** Neue optionale Felder → Minor-Bump; neue Pflichtfelder oder geänderte Semantik bestehender Felder → Major-Bump.
3. **Stabilität von Fehlercodes.** Bereits veröffentlichte Codes nicht wiederverwenden, Semantik nicht ändern; neue dürfen hinzukommen.
4. **Universal_Instruction-Kategorien.** Bestehende 8 Kategorien dürfen ihre Nummer nicht ändern; neue Kategorien nutzen reservierte Bits.
5. **Veraltungsfluss.** Erst mit `deprecation_notice` markieren; Laden gibt einen Hinweis aus, ist aber weiter erlaubt; Entfernung erst nach mindestens einer Major-Version.

## 8.8 Tooling-Empfehlungen

Zur Unterstützung langfristiger Evolution empfehlen wir:

- **Statische Abhängigkeitsrichtungs-Prüfung.** Verbieten, dass die Runtime einen Platform_Adapter-Typ importiert oder in Loader-Interna greift. Umsetzbar mit ArchUnit / dep-cruiser / eigenen Lints.
- **PBT-Baseline-Suite.** Die Korrektheits-Eigenschaften des Blueprints (Round-Trip-Äquivalenz, Zustandsübergänge, Fehlerketten, Capability-Mengenalgebra …) als sprachübergreifende Conformance-Suite festlegen, damit Runtime_Implementations sich selbst prüfen können.
- **Manifest-CBOR-Validator.** Verifizieren, dass Manifeste deterministisches CBOR nutzen; nicht-deterministische Codierungen sind nicht konform.
- **Adapter-Deskriptor-Health-Check.** Bei Adapter-Registrierung sofort `supported_capabilities` und `supported_instructions` auf Konsistenz prüfen (z. B. minimaler Abschluss von Capability-Bits gegen anweisungsbenötigte Capabilities).

## 8.9 Phasen-Roadmap (Übersicht)

| Phase | Schwerpunkte | Hauptbetroffene Schicht | Kompatibilitätswirkung |
| --- | --- | --- | --- |
| Phase 1 (heute) | Drei-Schichten-Verträge, Interpretation, vier eingebaute Adapter, Signaturen, partielles Section-Laden, Eager/Lazy, BuF_Source-Abstraktion | Drei Schichten + Querschnitt | — |
| Phase 2 | ExecutionStrategy / JIT / AOT | Runtime | kompatibel (Erweiterungen) |
| Phase 3 | Artefaktübergreifende Layer / Distribution / lokaler Cache | Loader | kompatibel (Minor) |
| Phase 4 | Geteilter Speicher / IPC / Abhängigkeitsgraph | Runtime + Loader | kompatibel (Erweiterungen) |
| Phase 5 | Multi-Knoten-Kollaboration | Adapter (Remote-Versand) | kompatibel (Erweiterungen) |

Designdokumente späterer Phasen werden als eigene Blueprints veröffentlicht und teilen Terminologie und Verträge mit diesem Blueprint.
