# Fayger クイックスタート

**Fayger** は Fay（`iFay` および `coFay`）向けのマルチプラットフォーム仮想実行環境（Virtual Runtime Environment）です。可搬性のある **BuF（Built-up Fay）** アーティファクトと宣言的な **FayManifest** 構成マニフェストを受け取り、デスクトップ・サーバー・ブラウザ・組み込みホスト端末にわたり一貫した検証、ライフサイクル管理、機能制限付きアダプター実行を提供します。

> 用語規定：**ホスト（Host）**は端末デバイスまたはアプリケーションプロセスのみを指し、自然人は常に **Human Prime（人間プロトタイプではなく人類原型）** と呼びます。

## コア 3 層アーキテクチャ

1. **ローダー層（Loader Layer）**：5 セグメント構造の `BuF` を解析し、ダイジェスト・Ed25519 署名・バージョン互換性を検証します。
2. **ランタイム層（Runtime Layer）**：言語中立な `Runtime_Interface` とクォータ分離を提供し、`fayger-conformance` スイートによって実装間の振る舞い等価性を検証します。
3. **アダプター層（Adapter Layer）**：`Universal_Instruction` をホスト機能へ変換し、CAP 認可ゲートと署名済み `Ego_Package` 検証のもとで制御された操作を実行します。

```bash
fayger version
fayger conformance
fayger demo live-ifay
```

詳細は同ディレクトリの `blueprint/` 各章を参照してください。
