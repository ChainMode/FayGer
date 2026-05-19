# Chapitre 1. Introduction

## 1.1 Contexte

L'écosystème Fay a besoin d'un socle d'exécution qui masque les différences entre terminaux et hôtes, et qui permette à un même artefact Fay de tourner de manière cohérente sur ordinateurs de bureau, serveurs, navigateurs et hôtes embarqués. Ce socle, c'est **Fayger**.

Par analogie :

- Ce que la JVM est au bytecode Java, **Fayger** l'est à BuF.
- Ce que Docker / OCI Runtime est aux images de conteneur, **Fayger** l'est à BuF.

Fayger ne dicte pas comment la sémantique de Fay s'écrit. Il dicte seulement comment un artefact Fay est chargé, vérifié, planifié et exécuté, et comment il reste isolé de l'hôte.

## 1.2 Positionnement

Fayger est un **environnement d'exécution virtuel (Virtual Runtime Environment)** :

- Il accepte en entrée une entité empaquetée appelée **BuF (Built-up Fay)**.
- Il effectue la lecture, la vérification structurelle, la négociation de version, la vérification de signature et le chargement.
- Il exécute le BuF de manière interprétée dans l'une des implémentations linguistiques de la couche d'exécution.
- Via la couche d'adaptation, il traduit les « instructions universelles » produites pendant l'exécution vers les capacités système du terminal cible.

Du point de vue utilisateur : on confie un BuF à Fayger, et Fayger le fait tourner.

## 1.3 Principes de conception

La première phase suit ces priorités :

1. **Portabilité d'abord.** Un BuF se compile une fois et tourne sur tout hôte adapté. Les couches d'exécution et d'adaptation sont découplées via une *Universal_Instruction* stable.
2. **Implémentations multi-langage.** La couche d'exécution expose une *Runtime_Interface* neutre vis-à-vis du langage, permettant à des implémentations en Rust, Go, TypeScript, etc. de coexister et d'être substituées, sans s'enfermer dans un écosystème.
3. **Correction démontrable.** Le parsing et la sérialisation BuF satisfont l'équivalence aller-retour ; la machine à états du cycle de vie satisfait des propriétés de transitions légales ; le comportement multi-plateforme satisfait l'équivalence de séquences. Ces propriétés sont formulées de façon vérifiable par des tests basés sur les propriétés (PBT).
4. **Sûr et observable par défaut.** Valeurs par défaut sûres (mode signature obligatoire disponible), silencieuses (événements de débogage désactivés) et erreurs structurées (modèle unifié avec étiquette de couche source et chaîne de causes).

## 1.4 Vue d'ensemble en trois couches

Fayger est divisé verticalement en trois couches :

- **Loader Layer.** Frontière entre BuF et le monde extérieur. En charge de la lecture, de l'écriture, de la vérification et de la négociation de version.
- **Runtime Layer.** Exécuteur de la sémantique de BuF. Supporte plusieurs implémentations linguistiques, interchangeables via un contrat unifié.
- **Adapter Layer.** Frontière entre Fayger et l'hôte. En charge de la traduction bidirectionnelle entre instructions universelles et appels système.

Chaque couche a un contrat externe clair et une division interne du travail. Les détails suivent au chapitre suivant.

## 1.5 Correspondance avec les systèmes existants

| Système existant | Équivalent Fayger |
| --- | --- |
| JVM ClassLoader → Verifier → ExecutionEngine | Loader Layer → Runtime Layer |
| Class file JVM (magic / version / constant pool) | Artefact BuF (Header / Manifest / Sections / Trailer) |
| OCI Image Manifest + Layers | BuF_Manifest + Sections |
| OCI Runtime Spec | Runtime_Interface |
| containerd vs runc | Orchestration Runtime_Layer vs exécution Runtime_Implementation |
| WASI capability-based security | Réduction des capacités d'Adapter_Layer |
| JNI / extism host functions | Catégorie `host` de Universal_Instruction |

## 1.6 Glossaire

- **Fayger.** L'environnement d'exécution virtuel défini par ce blueprint.
- **BuF (Built-up Fay).** Entité distribuable et chargeable de Fay. Unité d'entrée de Fayger.
- **BuF_Manifest.** Catalogue de métadonnées au sein d'un BuF (versions, point d'entrée, dépendances, capacités, quotas, signature, …).
- **Loader_Layer / Runtime_Layer / Adapter_Layer.** Les trois couches de Fayger.
- **Runtime_Interface.** Contrat externe stable de la couche d'exécution.
- **Runtime_Implementation.** Implémentation linguistique concrète de la couche d'exécution.
- **Platform_Adapter.** Adaptateur ciblant une terminale / un OS dans la couche d'adaptation.
- **Universal_Instruction.** Instruction indépendante de la plate-forme entre les couches d'exécution et d'adaptation.
- **BuF_Instance.** Entité BuF chargée dans la couche d'exécution, exécutable ou en cours d'exécution.
- **Lifecycle_State.** État du cycle de vie d'une BuF_Instance.
- **Host_Environment.** La terminale, l'OS ou le processus hôte où Fayger tourne réellement.
- **BuF_Source.** Abstraction du backend de stockage pour BuF (fichier local, réseau, stockage objet, backends utilisateur). Expose au minimum la lecture à accès aléatoire.
- **LoadProfile.** Profil d'environnement d'exécution fourni par l'appelant au chargement. Pilote la sélection des Sections. Inclut au minimum classe de terminale, ensemble de capacités, limites de taille et bits de fonctionnalités runtime.
- **SectionVisibility.** Niveau de visibilité d'une Section : `Required` (absence = échec) ou `Optional` (absence = dégradation).
- **LoadStrategy.** Stratégie de fetch du loader : `Eager` (lit toutes les Sections sélectionnées d'avance) ou `Lazy` (lit uniquement en-tête + index ; les corps de Section sont récupérés au premier accès).

## 1.7 Organisation du blueprint

Les chapitres suivants se déroulent dans cet ordre :

- Chapitre 2. Architecture générale
- Chapitre 3. Couche de chargement
- Chapitre 4. Couche d'exécution
- Chapitre 5. Couche d'adaptation
- Chapitre 6. Erreurs et observabilité
- Chapitre 7. Modèle de sécurité
- Chapitre 8. Feuille de route
