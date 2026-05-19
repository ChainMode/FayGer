# Kapitel 1. Einleitung

## 1.1 Hintergrund

Das Fay-Ökosystem braucht eine Ausführungsbasis, die Unterschiede zwischen Endgeräten und Hosts verbirgt und dafür sorgt, dass dasselbe Fay-Artefakt auf Desktops, Servern, Browsern und eingebetteten Hosts konsistent läuft. Diese Basis ist **Fayger**.

In Analogie:

- Was die JVM für Java-Bytecode ist, ist **Fayger** für BuF.
- Was Docker / OCI Runtime für Container-Images ist, ist **Fayger** für BuF.

Fayger schreibt nicht vor, wie die Semantik von Fay selbst geschrieben wird. Es schreibt nur vor, wie ein Fay-Artefakt geladen, geprüft, geplant und ausgeführt wird und wie es vom Host getrennt bleibt.

## 1.2 Positionierung

Fayger ist eine **virtuelle Laufzeitumgebung (Virtual Runtime Environment)**:

- Sie nimmt eine als **BuF (Built-up Fay)** bezeichnete Paketentität als Eingabe.
- Sie führt Lesen, Strukturprüfung, Versionsverhandlung, Signaturprüfung und Laden aus.
- Sie führt das BuF in einer von mehreren Sprach-Implementierungen der Laufzeitschicht interpretiert aus.
- Sie übersetzt während der Ausführung erzeugte „universelle Anweisungen" über die Adapterschicht in die Systemfähigkeiten des Zielgeräts.

Aus Anwendersicht: BuF an Fayger übergeben, Fayger lässt es laufen.

## 1.3 Designprinzipien

Die erste Phase folgt diesen Prioritäten:

1. **Portabilität zuerst.** Ein BuF wird einmal kompiliert und läuft auf jedem angepassten Host. Laufzeit- und Adapterschicht sind über eine stabile *Universal_Instruction* entkoppelt.
2. **Mehrsprachige Implementierungen.** Die Laufzeitschicht stellt einen sprachneutralen *Runtime_Interface* bereit. Rust, Go, TypeScript und andere Implementierungen können nebeneinander existieren und ausgetauscht werden, ohne sich auf ein einzelnes Ökosystem festzulegen.
3. **Beweisbare Korrektheit.** BuF-Parsen und -Serialisieren erfüllen Round-Trip-Äquivalenz, der Lebenszyklus-Zustandsautomat erfüllt Eigenschaften legaler Übergänge, und plattformübergreifendes Verhalten erfüllt Sequenzäquivalenz. Diese Eigenschaften sind so formuliert, dass sie durch Property-Based Testing (PBT) verifiziert werden können.
4. **Standardmäßig sicher und beobachtbar.** Sichere Voreinstellungen (Modus „erzwungene Signatur" verfügbar), stille Voreinstellungen (Debug-Ereignisse aus) und strukturierte Fehler (einheitliches Fehlermodell mit Quellschicht-Tag und Ursachenkette).

## 1.4 Drei-Schichten-Überblick

Fayger ist vertikal in drei Schichten unterteilt:

- **Loader Layer.** Die Grenze zwischen BuF und der Außenwelt. Zuständig für Lesen, Schreiben, Prüfung und Versionsverhandlung.
- **Runtime Layer.** Der Ausführer der BuF-Semantik. Unterstützt mehrere Sprach-Implementierungen, austauschbar über einen einheitlichen Schnittstellenvertrag.
- **Adapter Layer.** Die Grenze zwischen Fayger und Host. Zuständig für die bidirektionale Übersetzung zwischen universellen Anweisungen und Systemaufrufen.

Jede Schicht hat einen klaren externen Vertrag und eine interne Aufgabenteilung. Die Details folgen im nächsten Kapitel.

## 1.5 Abbildung auf bestehende Systeme

| Bestehendes System | Entsprechung in Fayger |
| --- | --- |
| JVM ClassLoader → Verifier → ExecutionEngine | Loader Layer → Runtime Layer |
| JVM Class File (magic / version / constant pool) | BuF-Artefakt (Header / Manifest / Sections / Trailer) |
| OCI Image Manifest + Layers | BuF_Manifest + Sections |
| OCI Runtime Spec | Runtime_Interface |
| containerd vs. runc | Runtime_Layer-Orchestrierung vs. Runtime_Implementation-Ausführung |
| WASI Capability-based Security | Capability-Trimming der Adapterschicht |
| JNI / extism Host Functions | Die Kategorie `host` von Universal_Instruction |

## 1.6 Glossar

- **Fayger.** Die in diesem Blueprint definierte virtuelle Laufzeitumgebung.
- **BuF (Built-up Fay).** Die verteilbare, ladbare Entität von Fay. Eingabeeinheit von Fayger.
- **BuF_Manifest.** Der Metadatenkatalog innerhalb eines BuF (Versionen, Einstieg, Abhängigkeiten, Capabilities, Quotas, Signatur, …).
- **Loader_Layer / Runtime_Layer / Adapter_Layer.** Die drei Schichten von Fayger.
- **Runtime_Interface.** Der stabile externe Vertrag der Laufzeitschicht.
- **Runtime_Implementation.** Eine konkrete Sprach-Implementierung der Laufzeitschicht.
- **Platform_Adapter.** Ein Adapter für ein Endgerät / Betriebssystem in der Adapterschicht.
- **Universal_Instruction.** Die plattformunabhängige Anweisung zwischen Laufzeit- und Adapterschicht.
- **BuF_Instance.** Eine in die Laufzeitschicht geladene BuF-Entität, ausführbar oder in Ausführung.
- **Lifecycle_State.** Der Lebenszykluszustand einer BuF_Instance.
- **Host_Environment.** Das Endgerät, Betriebssystem oder der Host-Anwendungsprozess, in dem Fayger tatsächlich läuft.
- **BuF_Source.** Speicher-Backend-Abstraktion für BuF (lokale Datei, Netzwerk, Object Storage, benutzerdefinierte Backends). Mindestens Random-Access-Read.
- **LoadProfile.** Das vom Aufrufer beim Laden bereitgestellte Laufzeitprofil. Steuert die Section-Auswahl. Enthält mindestens Geräteklasse, Capability-Set, Größenobergrenzen und Laufzeit-Feature-Flags.
- **SectionVisibility.** Die Sichtbarkeitsstufe einer Section: `Required` (Fehlen führt zu Fehler) oder `Optional` (Fehlen führt zu Degradation).
- **LoadStrategy.** Ladestrategie des Loaders: `Eager` (alle ausgewählten Sections vorab lesen) oder `Lazy` (nur Header + Index lesen; Section-Bodies bei erstem Zugriff holen).

## 1.7 Aufbau des Blueprints

Die folgenden Kapitel sind so geordnet:

- Kapitel 2. Gesamtarchitektur
- Kapitel 3. Loader-Schicht
- Kapitel 4. Runtime-Schicht
- Kapitel 5. Adapter-Schicht
- Kapitel 6. Fehler und Beobachtbarkeit
- Kapitel 7. Sicherheitsmodell
- Kapitel 8. Roadmap
