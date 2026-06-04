<!-- Mis à jour : 2026-05-26 -->

# Le runtime, c'est la gouvernance : comment FayGer garde l'IA maîtrisable et auditable sur n'importe quel terminal

Quand on parle de sûreté de l'IA, le premier réflexe est souvent l'« alignement du modèle ».  
C'est important, mais ça n'est pas toute l'histoire.

À partir du moment où l'IA prend la main sur des terminaux, opère des logiciels à votre place et fournit des services publics pour une organisation, ce qui peut vraiment plonger la société dans la panique, ce n'est pas une hallucination ponctuelle, c'est la perte de contrôle systémique :  
on ne sait pas où elle tourne, on ne sait pas quels droits elle a obtenus, on ne sait pas ce qu'elle vient de faire, et surtout on ne sait ni à qui demander des comptes, ni comment l'arrêter, ni comment relire l'incident, ni comment porter réclamation.

J'appelle ce risque **« l'action en état libre »**.  
Dès qu'on laisse une IA agir en état libre, aucun argument produit, aucune phrase du genre « nous avons pris des mesures de sûreté » ne tiendra au prochain incident.

Voilà pourquoi je le redis : **on ne peut pas laisser l'IA livrée à elle-même ; elle doit agir sous tutelle humaine.**  
Cela ressemble à une posture, mais d'un point de vue ingénierie cela exige quelque chose de très concret : un socle d'exécution cohérent à travers les terminaux, les langages et les fournisseurs, qui fasse de la tutelle, de l'isolation, de l'audit et de la révocation des valeurs par défaut.  
C'est exactement le rôle de FayGer.

## 1. Pourquoi le « runtime » devient une infrastructure de gouvernance à l'ère de l'IA

À l'ère logicielle classique, le runtime était surtout un confort d'ingénierie :  
la JVM permettait à Java d'être compilé une fois et exécuté partout ; Docker faisait tourner les images de manière stable d'une machine à l'autre ; WASM/WASI faisait s'exécuter le code dans un bac à sable.

À l'ère de Fay (iFay/coFay), le runtime cesse d'être un simple confort : il devient infrastructure de gouvernance. Parce que Fay ne se contente pas de calculer, **elle agit sur le monde** :  
envoyer des messages, soumettre des formulaires, piloter des terminaux, appeler du matériel, déclencher des paiements, ordonner des drones... Autant d'actions à conséquences réelles.

Si vous laissez chaque Fay tourner « chacune dans son coin » sur des terminaux, des hôtes et des chaînes d'appels système différents, vous obtenez le pire :  
**une sémantique de tutelle fragmentée.**  
Sur un appareil le frein agit, sur un autre non ; dans un système l'audit existe, dans l'autre il n'y a que des fragments de logs ; dans une version les droits sont restreints, dans une autre ils ont été élargis en silence.

Ce n'est pas seulement un problème d'ingénierie, c'est un problème social.  
Parce qu'au moment de l'incident, ce sont précisément ces « écarts d'environnement » qui déchirent la chaîne de responsabilité.

## 2. La place de FayGer : à la manière de la JVM ou de Docker, mais pour une « exécution gouvernable »

FayGer en une phrase : **l'environnement d'exécution virtuel de l'écosystème Fay.**  
Il accepte un artefact Fay distribuable, chargeable et vérifiable (le BuF) et l'exécute de manière cohérente sur des terminaux différents.

Mais ce qui m'importe, ce n'est pas « est-ce que ça tourne », c'est **comment** ça tourne.  
La promesse centrale de FayGer tient en quatre mots : **cohérent, isolé, prouvable, observable.**

Par analogie :

- Pour le bytecode Java : la JVM se moque de votre logique métier, mais elle dicte le chargement, la vérification, l'exécution et l'isolation.
- Pour les images de conteneur : Docker / OCI Runtime se moque de ce que fait votre application, mais il fixe la structure d'image, les permissions, les quotas de ressources et les limites d'exécution.

FayGer agit pareillement. Il ne dit pas comment se trace la frontière de capacités d'une Fay (c'est l'affaire du protocole et des skills). Il dit : **lorsqu'une Fay est livrée à la société, dans quelle « posture gouvernable » doit-elle s'exécuter.**

## 3. Pourquoi la structure en trois couches compte : Loader / Runtime / Adapter

FayGer se découpe en trois couches, et ce découpage est crucial pour la gouvernance :

1) **Couche de chargement (Loader)**  
La frontière entre BuF et le monde extérieur. Elle se charge de la lecture, de la validation structurelle, de la négociation de version, de la vérification de signature et de la stratégie de chargement (Eager/Lazy).  
Je la vois comme **les douanes** : si la structure ne convient pas, on n'entre pas ; sans signature, on n'entre pas ; si la version n'est pas compatible, on n'entre pas.

2) **Couche d'exécution (Runtime)**  
L'exécutant de la sémantique BuF. Elle permet à plusieurs implémentations de langages (Rust / Go / TypeScript...) d'être interchangeables via une interface unifiée.  
Cela règle un problème souvent ignoré : si on lie le runtime à un seul écosystème de langage, on lie la gouvernance à une seule entreprise ou à une seule chaîne d'outils, ce qui finit en monopole de fait.

3) **Couche d'adaptation (Adapter)**  
La frontière entre FayGer et le terminal hôte. Elle traduit les Universal Instructions en appels système et, dans l'autre sens, transforme les retours du système en événements auditables.  
Je la vois comme **un sas de permissions** : ce que le terminal accepte d'accorder, en quelle quantité et avec quelle granularité, doit se négocier et se borner ici.

L'intérêt de ces trois couches : la « sémantique de gouvernance » vit dans des couches stables, pas dispersée dans les détails d'ingénierie de chaque terminal.  
La gouvernance ne peut pas reposer sur la bonne volonté de chaque équipe ; elle doit être imposée par l'architecture.

## 4. « Maîtrisable » n'est pas un bouton, c'est un ensemble de valeurs par défaut

Quand je dis « l'IA doit agir sous tutelle », je ne demande pas un interrupteur d'IHM.  
Je demande un ensemble de valeurs par défaut impossibles à contourner, et FayGer est le bon endroit pour les héberger.

Quelques points qui devraient à mon sens être figés comme défauts du runtime :

### 4.1 Négociation de capacités et moindre privilège

Les capacités du terminal doivent être déclarées, négociées et accordées, pas « si le programme peut l'appeler, alors il appelle ».  
Si un BuF n'a pas reçu la capacité d'écrire dans le système de fichiers, il ne doit pas pouvoir écrire « au passage » via une faille de l'OS.

Cela devient particulièrement sensible dès que la société dépend de coFay pour des services publics :  
si les limites de droits d'un rôle public sont floues, l'efficacité se transforme en pouvoir.

### 4.2 Quotas de ressources et rayon d'effet

Les quotas CPU / mémoire / réseau / stockage d'une Fay sont, au fond, son rayon d'effet.  
On ne peut pas miser sur « le modèle se modérera tout seul » ; il n'y a que les limites dures au niveau runtime qui tiennent.

### 4.3 Observabilité : l'audit ne doit pas être un patch a posteriori

Dans beaucoup de systèmes, l'audit est rajouté après un incident.  
Pour Fay, l'audit doit être une capacité par défaut ; sinon, on ne pourra jamais remonter la responsabilité jusqu'au bout.

Mon approche préférée : un runtime qui fournit un modèle d'erreur unifié, avec marquage de la couche d'origine et chaîne de causes, plus des erreurs structurées ; et qui transforme les événements d'exécution clés en un flux d'audit auquel on peut s'abonner, qu'on peut agréger et qu'on peut figer.  
Silencieux par défaut (il ne dérange personne), mais entièrement consultable au besoin.

### 4.4 « Stopper l'effet vers l'extérieur » est prioritaire

Quand la relation de tutelle ne tient plus, quand le responsable est injoignable, ou quand le système entre dans un état incertain, je préfère arrêter Fay.  
Non par prudence, mais par respect pour la structure de responsabilité :  
**mieux vaut ne pas finir la tâche que continuer à agir vers l'extérieur quand plus personne n'est responsable.**

Si ce genre de règle se disperse dans le code métier, les KPI finiront par la grignoter ; placée dans le runtime, elle a une chance de devenir une règle sociale.

## 5. FayGer face à iFay/coFay : il ne résout pas tout, mais il décide si le plancher tient

Je ne vois pas FayGer comme une solution miracle.  
Identité, frontières de persona, sémantique de protocole, mécanismes de recours, consensus sur les contributions... chacun a besoin de sa propre couche.

Mais FayGer tranche une question plus fondamentale :  
quand vous avez écrit ces idées dans des documents, des protocoles et des manifestes, **tiennent-elles encore quand elles atteignent un terminal réel ?**

En Phase 1, vous pouvez laisser iFay simuler des actions d'IHM.  
En Phase 2, vous pouvez prendre la main sur les terminaux via CAP/DTP.  
En Phase 3, vous pouvez appeler des skills en réseau via des protocoles distants.  
En Phase 4, vous pouvez faire collaborer iFay et coFay.  
En Phase 5, vous devrez peut-être même discuter de la répartition de la valeur dans une ère post-monétaire.

Mais quelle que soit la phase, la phrase de base ne devrait pas changer :  
**on ne peut pas laisser l'IA livrée à elle-même ; elle doit agir sous tutelle humaine.**

Le rôle du runtime, c'est de transformer cette « phrase humaine » en un fait que le système ne peut pas contourner.

## 6. Conclusion : la peur de demain n'est pas « l'IA est puissante », c'est « l'IA est puissante et ingouvernable »

Aujourd'hui, beaucoup d'angoisses autour de l'IA tournent autour du chômage, de l'appauvrissement et de l'effondrement du sens.  
Je comprends ces émotions.

Mais si nous allons vraiment vers une ère d'abondance productive, le droit de vivre ne devrait plus être l'otage du travail.  
Ce qui décidera de la stabilité d'une société, c'est la gouvernance : la responsabilité est-elle traçable, les recours ont-ils une porte d'entrée, les services publics sont-ils audités, le privilège et la voix sont-ils corrélés à la contribution.

Et la première étape de la gouvernance, c'est de rendre gouvernable l'exécution même de l'IA.  
À mes yeux, FayGer est la fondation qu'on ne peut pas contourner.

---

## Documents associés
- FayGer | Introduction (EN) : https://ifay.ai/en/docs/FayGer/blueprint/01-introduction  
- iFay | Feuille de route (EN) : https://ifay.ai/en/docs/iFay/blueprint/04-Roadmap  
- iFay | Feuille de route (chinois) : https://ifay.ai/zh-CN/docs/iFay/blueprint/04-%E8%B7%AF%E7%BA%BF%E5%9B%BE  
