# Chapitre 8. Feuille de Route

Ce chapitre définit les frontières de la première phase et trace l'évolution selon les directions connues. Chaque élément note son périmètre d'impact sur l'architecture actuelle pour aider les designs futurs à converger.

## 8.1 Périmètre de la première phase

Une livraison de première phase satisfait au moins :

- Contrats stables pour les trois couches (Loader / Runtime / Adapter).
- Format d'artefact BuF en cinq parties avec Manifest / Section_Index en CBOR déterministe.
- Chaîne de chargement : Read(Header/Manifest/Index) → Parse → Verify(Structural) → Verify(Digest of Header/Manifest/Index) → Verify(Signature) → Negotiate Version → Select(Sections by LoadProfile) → Resolve → Read(Selected Section Bodies)? → HandOff.
- Abstraction BuF_Source supportant tout backend (local, réseau, stockage objet, défini par l'utilisateur).
- Chargement partiel (sélection par profil) et chargement paresseux (Eager / Lazy LoadStrategy), couvrant les besoins de taille et de récupération à l'exécution des terminales contraintes (drones, caméras).
- Chemin d'exécution interprété ; machine à états du cycle de vie de BuF_Instance et isolation des défaillances.
- 8 catégories d'Universal_Instruction et quatre Platform_Adapters intégrés (bureau / serveur / navigateur / In-App).
- Modèle d'erreurs unifié et bus d'événements d'observabilité ; les erreurs du loader portent une étiquette `phase`.
- Vérification de signature, mode signature obligatoire et visibilité des mises à jour de racines.

## 8.2 Non-objectifs de la première phase

Pour borner la complexité, ne sont pas livrés :

- Mémoire partagée et IPC entre BuF_Instances.
- Compilation JIT et génération de code machine.
- Planification distribuée et collaboration multi-nœuds de Fayger.
- Distribution différentielle de BuF et caches en couches inter-artefacts (le chargement partiel couvre la sélection par Section dans un artefact ; la réutilisation différentielle inter-artefacts et un protocole Distribution sont remis à plus tard).
- Dépôt distant intégré / protocole Distribution.

Un non-objectif ne signifie pas « jamais nécessaire », mais « pas dans la surface de contrat aujourd'hui ». Les phases ultérieures ne devront pas casser les contrats existants.

## 8.3 Phase 2 : performance d'exécution

Direction : ajouter des chemins d'exécution plus efficaces sans casser Runtime_Interface.

### 8.3.1 Abstraction ExecutionStrategy

Extraire « comment exécuter la sémantique BuF » en stratégie :

```idl
interface ExecutionStrategy {
  prepare(obj: BuFObject) -> PreparedExecutable
  step(exec: PreparedExecutable, ctx: ExecutionContext) -> StepResult
}
```

La phase 1 livre `Interpreter` comme une stratégie. Plusieurs peuvent coexister ensuite :

- `JIT` : compiler la représentation intermédiaire de la code section en code machine.
- `AOT` : produire le code machine au chargement (adapté aux releases).
- `HybridTier` : interpréter → sonder → compiler.

Le choix de stratégie est interne à une Runtime_Implementation et n'affecte pas le contrat externe de Runtime_Interface.

### 8.3.2 Observabilité des performances

Nouvelles catégories :

- `ExecutionMetric` : échantillonnage interprétation / compilation / GC.
- `TierTransition` : passage entre niveaux d'exécution par paliers.

Les stratégies concrètes ne sont pas exposées, mais leur effet est observable.

## 8.4 Phase 3 : distribution et cache

### 8.4.1 Couches et chunks inter-artefacts

La phase 1 supporte déjà la lecture à la demande à granularité Section dans un BuF (Lazy). Plus tard, on introduit le partage par couches inter-artefacts :

- Les ressources partagées entre BuFs (bibliothèque standard, poids de modèles) sont extraites en couches indépendantes.
- Le Manifest gagne un `layer_refs` optionnel référant des Sections d'autres artefacts.
- Un protocole de distribution récupère les ressources partagées ; un cache local atteint permet la réutilisation entre BuFs.

Cela demande des champs optionnels dans le Manifest et donc un bump mineur de `schema_version`. Les BuFs existants ne sont pas affectés (chemin de compatibilité §8.7).

### 8.4.2 Protocole de distribution

Introduction d'un protocole léger (inspiré d'OCI Distribution Spec) :

- Trois étapes : `discover` / `pull` / `verify`.
- Le protocole ne vit pas dans le processus Fayger ; un composant indépendant l'implémente. Fayger ne consomme que les chunks produits.

### 8.4.3 Cache local

Pour les BuFs déjà chargés et signés, mettre en cache le résultat de parsing du Manifest et des Sections entre processus. Lors d'un hit, la signature est revérifiée (anti-empoisonnement).

## 8.5 Phase 4 : collaboration entre instances

### 8.5.1 Mémoire partagée (contrôlée)

Permettre à plusieurs BuF_Instances dans un même Fayger de partager une région mémoire :

- Déclarer la capacité `shared_memory` explicitement dans le Manifest.
- La couche d'exécution alloue les régions de manière centralisée et les expose selon les concessions.
- Les régions partagées sont sous surveillance de quotas ; toute violation déclenche la suspension.

### 8.5.2 Protocole IPC

Messagerie inter-instance (même processus ou même Fayger) :

- Exprimée comme une nouvelle catégorie `ipc` d'Universal_Instruction.
- Refusée par défaut ; le Manifest doit déclarer l'ensemble d'IDs des correspondants ou les topics.

### 8.5.3 Assemblage de dépendances dans un même Fayger

Permettre à un BuF d'en référencer d'autres en dépendance :

- La phase `Resolve` supporte la résolution récursive.
- Le graphe de dépendances est exprimé via le champ `dependencies` du Manifest.
- Les dépendances cycliques sont rejetées en `Resolve`.

## 8.6 Phase 5 : distribué

Direction : collaboration multi-nœuds de Fayger. Hors phase 1, mais on garde de la marge pour :

- Les IDs de BuF_Instance doivent être uniques et adressables à travers les nœuds.
- Les timestamps de Lifecycle Events doivent s'aligner avec les systèmes de tracing externes.
- L'encodage TLV d'Universal_Instruction supporte déjà le transport réseau ; le « dispatch distant » peut être un Adapter à part entière.

## 8.7 Stratégie de compatibilité

Toute évolution doit suivre ces règles ; sinon c'est un changement cassant :

1. **Version de Runtime_Interface.** Bump SemVer à l'ajout de méthodes ou au changement de sémantique. BuF_Manifest exprime sa dépendance minimale via `runtime_interface_min`.
2. **Version de schema BuF.** Ajout de champs optionnels → bump mineur ; ajout de champs requis ou changement de sémantique d'un champ existant → bump majeur.
3. **Stabilité des codes d'erreur.** Les codes publiés ne sont pas réutilisés ni modifiés ; on peut en ajouter.
4. **Catégories Universal_Instruction.** Les 8 actuelles ne réutilisent pas leur numéro ; les nouvelles catégories utilisent les bits réservés.
5. **Flux de dépréciation.** Marquer d'abord avec `deprecation_notice` ; au chargement, un avis est émis mais le chargement reste autorisé ; suppression au moins après une majeure.

## 8.8 Recommandations d'outillage

Pour soutenir l'évolution à long terme, nous recommandons :

- **Vérification statique de direction de dépendances.** Interdire à la couche d'exécution d'importer un type Platform_Adapter ou d'aller chercher des types internes du loader. Implémentable avec ArchUnit / dep-cruiser / lints maison.
- **Suite de tests basés sur les propriétés (PBT) de référence.** Fixer les propriétés de correction du blueprint (équivalence aller-retour, transitions d'état, chaînes d'erreurs, algèbre des capacités, …) en Conformance Suite multi-langage que toute Runtime_Implementation peut auto-vérifier.
- **Validateur CBOR de Manifest.** Vérifier que le Manifest utilise CBOR déterministe ; les encodages non déterministes ne sont pas conformes.
- **Health-check de descripteur d'Adapter.** À l'enregistrement, vérifier immédiatement la cohérence de `supported_capabilities` et `supported_instructions` (par ex. fermeture minimale entre bits de capacités et capacités requises par instruction).

## 8.9 Synthèse de la feuille de route

| Phase | Directions clés | Couches principales | Impact compatibilité |
| --- | --- | --- | --- |
| Phase 1 (actuelle) | Contrats trois couches, exécution interprétée, quatre adaptateurs intégrés, signatures, chargement partiel par Section, Eager/Lazy, abstraction BuF_Source | Trois couches + transverse | — |
| Phase 2 | ExecutionStrategy / JIT / AOT | Runtime | Compatible (additions) |
| Phase 3 | Couches inter-artefacts / Distribution / cache local | Loader | Compatible (mineur) |
| Phase 4 | Mémoire partagée / IPC / graphe de dépendances | Runtime + Loader | Compatible (additions) |
| Phase 5 | Collaboration multi-nœuds | Adapter (dispatch distant) | Compatible (additions) |

Les documents de design des phases ultérieures seront publiés comme blueprints autonomes et partageront avec le présent blueprint la même terminologie et les mêmes contrats.
