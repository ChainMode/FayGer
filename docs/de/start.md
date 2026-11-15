# Erste Schritte mit Fayger

**Fayger** ist die plattformübergreifende virtuelle Laufzeitumgebung (Virtual Runtime Environment) für Fay (`iFay` und `coFay`). Sie verarbeitet portable **BuF (Built-up Fay)**-Artefakte sowie deklarative **FayManifest**-Konfigurationen und stellt über Desktop-, Server-, Browser- und Embedded-Hosts hinweg eine einheitliche Verifikation, Lebenszyklussteuerung und abgesicherte Host-Adaption bereit.

> Terminologiehinweis: **Host** bezeichnet ausschließlich das Endgerät oder den Anwendungsprozess; die natürliche Person wird stets als **Human Prime** bezeichnet.

## Drei-Schichten-Architektur

1. **Loader-Schicht**: Prüft `BuF`-Artefakte, Prüfsummen, Ed25519-Signaturen und Versionskompatibilität.
2. **Runtime-Schicht**: Setzt den sprachneutralen `Runtime_Interface`-Vertrag und Ressourcenquoten um, geprüft durch die `fayger-conformance`-Suite.
3. **Adapter-Schicht**: Übersetzt `Universal_Instruction`-Befehle unter CAP-Autorisierung und zweistufiger `Ego_Package`-Signaturprüfung in kontrollierte Host-Operationen.

```bash
fayger version
fayger conformance
fayger demo live-ifay
```

Weitere Details finden Sie in den Kapiteln unter `blueprint/`.
