# Fayger 快速開始

**Fayger** 是面向 Fay（`iFay` 與 `coFay`）的多平台虛擬執行環境（Virtual Runtime Environment）。它負責接收可分發的 **BuF（Built-up Fay）** 製品與 **FayManifest** 宣告式裝配清單，在桌面、伺服器、瀏覽器與嵌入式宿主終端之間提供統一的載入校驗、執行生命週期管理、適配層能力裁剪與可稽核執行閉環。

> 術語邊界：本文件中**宿主（Host）**僅指被附身的終端設備或應用程序；自然人一律稱為**人類原型（Human Prime）**。

## 核心三層架構

1. **載入層（Loader Layer）**：解析五段式 `BuF` 製品，校驗摘要、Ed25519 簽章與執行介面版本約束。
2. **執行層（Runtime Layer）**：透過語言中立的 `Runtime_Interface` 管理生命週期遷移與資源配額隔離，並由 `fayger-conformance` 黑盒一致性套件驗證跨實作行為等價性。
3. **適配層（Adapter Layer）**：將平台無關的 `Universal_Instruction` 映射為宿主能力，並在生產級 CAP 授權門禁與已簽章 `Ego_Package` 雙層驗簽通過後執行受控工作區操作。

## 常用命令列入口

```bash
fayger version
fayger info
fayger conformance
fayger demo live-ifay
```

完整架構與分章設計請閱讀同目錄下的 `blueprint/` 章節。
