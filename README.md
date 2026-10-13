# HKJEBN-GYM-L-MEAL

樓上凍品價格比較工具 **gymL**，以 Rust 與原生 Win32 控制項製作的 Windows 可攜程式。

## 下載 v1.1

**[下載可攜 ZIP](https://github.com/nat510/HKJEBN-GYM-L-MEAL/releases/download/v1.1/gymL-portable-windows-x64.zip)** · [下載 gymL.exe](https://github.com/nat510/HKJEBN-GYM-L-MEAL/releases/download/v1.1/gymL.exe) · [完整發行頁](https://github.com/nat510/HKJEBN-GYM-L-MEAL/releases/tag/v1.1)

[SHA-256 校驗檔](https://github.com/nat510/HKJEBN-GYM-L-MEAL/releases/download/v1.1/SHA256SUMS.txt) · [第三方授權聲明](https://github.com/nat510/HKJEBN-GYM-L-MEAL/releases/download/v1.1/THIRD_PARTY_LICENSES.txt)

## 使用方式

1. Windows 10／11 x64：把 ZIP 解壓至可寫入的資料夾。
2. 執行 `gymL.exe`，不需安裝。
3. 每次開啟都在背景擷取商品、優惠及推廣；按 **F5** 可手動更新。
4. 搬移程式時，把旁邊的 `gymL_data` 一起搬移，即可保留偏好及快取。

```text
gymL.exe
gymL_data/
  catalog.json
  coupons.json
  promotions.json
  preferences.json
  images/
```

## 功能

- 分類：全部、海產、肉類、魚、雞、牛、豬、蝦。
- 搜尋、至Like價／特價／正價篩選、貴價及排除名稱篩選、最新異動篩選。
- 排序：預設、分組、價格由低至高／由高至低、會員折扣；價格排序優先使用會員價。
- 保留現價及來源生效價目；未標年份的日期保留原文，不猜測年份。
- **Ctrl+F** 聚焦搜尋；選取項目可查看完整名稱、單位及價目。
- 雙擊、Enter 或「查看圖片」開啟原生圖片視窗，支援適合視窗、1:1、捲動及前後切換；Escape 關閉。
- 優惠／推廣共用可編輯的隱藏名稱關鍵字，並可切換「顯示全部」。

金額為港元，價格以零售商為準。程式不記錄價格歷史，也不安裝排程工作。閒置時等待 Windows 訊息；圖片只在選取後載入。

## 資料與快取

資料儲存在 EXE 旁的 `gymL_data`。擷取期間保留可用資料；兩個商品分類完整成功後才替換商品快取。排隊、驗證、逾時、缺少價格或分頁不完整時保留上次完整資料並顯示原因。優惠及推廣各自保存成功結果，並顯示實際擷取時間。

每次開啟／手動更新的擷取作業完成後，清理滿 30 天未成功更新的已擷取快取；最新確認資料及偏好設定保留。圖片年齡以成功下載時間計算，查看不重設時間。圖片磁碟快取上限 32 MB，單張解碼圖片配置上限 64 MB。

程式直接讀取零售商資料；匿名會話 Cookie 只留在當次擷取的記憶體中。

## 原始碼與建置

需要 Windows、Rust MSVC 工具鏈及 Windows SDK。發行流程使用 Rust 1.99.0。

```powershell
git clone https://github.com/nat510/HKJEBN-GYM-L-MEAL.git
cd HKJEBN-GYM-L-MEAL
./scripts/build-portable.ps1
./scripts/verify-portable.ps1
```

建置會執行格式檢查、測試、Clippy 及最佳化編譯，產生 `gymL.exe` 與 `dist/gymL-portable-windows-x64.zip`。使用靜態 CRT、LTO、移除符號及匿名建置路徑；ZIP 只放程式、使用說明、授權聲明與空資料夾，不加入本機快取。

主要模組：`model.rs`（價格／篩選）、`parser.rs`（HTML 解析）、`fetch.rs`（WinHTTP 擷取）、`storage.rs`（快取／清理）、`ui.rs`（原生清單）、`viewer.rs`（圖片解碼／顯示）。

```powershell
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --locked -- -D warnings
```

GitHub Actions 在 `main` 更新及 pull request 時驗證 Windows 建置。手動執行 **gymL Windows release** 並指定版本標籤，即可發行經驗證的 EXE、可攜 ZIP、校驗檔及授權聲明。

## 授權

Copyright (C) 2026 JEBN-GYM-L-MEAL contributors.

程式原始碼及 EXE 採用 **GNU Affero General Public License 第 3 版或任何後續版本**（SPDX: AGPL-3.0-or-later），詳見 [LICENSE](LICENSE)。程式不提供任何擔保。相依套件保留各自授權，聲明隨可攜套件提供。

[本版本完整對應原始碼](https://github.com/nat510/HKJEBN-GYM-L-MEAL/tree/v1.1)／[原始碼 ZIP](https://github.com/nat510/HKJEBN-GYM-L-MEAL/archive/refs/tags/v1.1.zip) 包含建置腳本。

## 擷取識別

商品、優惠、推廣及圖片的來源請求使用 2026-10-13 核對的 Windows x64 Chrome 156 User-Agent：

    Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36

Windows 11 的 Chrome 仍使用 Windows NT 10.0 作為相容性識別，次要版本、組建及修補版本固定為 0。此版本採用固定字串。
