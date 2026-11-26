# Pi Config App：桌面設定編輯器

[Rune](https://github.com/anomalyco/rune) 原生桌面程式只處理 Pi 的使用者層級 `settings.json`。第一版可編輯三個欄位；目前範圍見[可編輯欄位](#可編輯欄位)，操作方式見 [README](../../README.md)。

## 問題

想從桌面調整 Pi 偏好的使用者，目前可在 Pi 裡用 `/settings` 修改常用設定，或直接編輯 JSON。使用自訂 agent 目錄時，還要先確認自己改的是 Pi 實際讀取的檔案。桌面程式應顯示檔案位置，讓使用者不必懂 JSON 就能修改幾個常用欄位，且不會遺失其他設定。

## 目標與非目標

- **目標：** 在單一視窗完成「確認路徑 → 修改設定 → 儲存」，不需要背景服務或網路。
- **非目標：** 不編輯專案的 `.pi/settings.json`、`auth.json`、`models.json` 或其他檔案。不提供原始 JSON 編輯器、套件管理或設定同步，也不替執行中的 Pi 套用變更。

## 需求與驗收方式

| 需求 | 驗收方式 |
| --- | --- |
| **R1：找到設定檔。** 啟動時讀取程式收到的 `PI_CODING_AGENT_DIR`。變數未設定或為空時使用 `~/.pi/agent/`；支援 `~/` 開頭的路徑，相對路徑以啟動時的工作目錄為準。視窗標題列右側以單行顯示 `settings.json` 路徑；點一下路徑即可複製完整路徑。自訂路徑讀取失敗時，不改讀預設路徑。 | 在測試用家目錄與自訂目錄放入不同的設定值，分別用未設定、空值、絕對路徑、`~/` 路徑和相對路徑啟動，核對畫面路徑及載入值。讓自訂路徑無法讀取，確認顯示的是該路徑的錯誤。 |
| **R2：用表單編輯。** 依「可編輯欄位」分組顯示表單，切換群組會保留編輯與捲動位置。未設定的欄位顯示 Pi 的內建預設值，例如 **medium (default)**；使用者也可清除欄位以恢復預設。若既有值不屬於表單支援的類型或選項，標示為不支援並保留該值，不影響其他欄位。 | 載入含自訂主題、未設定欄位及表單不支援的值的檔案。修改一個支援的欄位並儲存，確認只有該欄位改變。清除一個已設定的欄位，確認只移除對應的 JSON 鍵。 |
| **R3：明確儲存。** 只有使用者按「儲存」才寫檔；沒有變更時不寫檔。保留所有未編輯的鍵和值，包括巢狀值；縮排與鍵順序可以改變。成功後提示：已開啟的 Pi 可使用 `/reload` 重新載入。 | 修改後不儲存就關閉視窗，確認檔案不變。儲存後重新開啟 App，確認修改仍在；比對儲存前後的 JSON，確認其他鍵和值不變，也沒有補寫預設值。 |
| **R4：區分缺檔與讀取失敗。** 檔案不存在時顯示「尚未建立」，只在第一次儲存變更時建立所需目錄與檔案。若 JSON 無效、最外層不是物件或檔案無法讀取，顯示路徑和原因，不允許儲存覆蓋；修正後可重新讀取。 | 分別測試缺檔、壞掉的 JSON、JSON 陣列、讀取權限不足。只允許缺檔情境建立新檔；其餘情境的原檔不得變動。 |
| **R5：避免覆蓋外部變更或寫入半份檔案。** 儲存前若發現檔案自載入後被修改、建立或刪除，停止寫入並提示重新讀取。重新讀取會捨棄未儲存變更，須先確認。若寫入失敗，保留原檔與畫面中的編輯，並顯示失敗原因。 | 開啟 App 後用另一個程式更改、建立或刪除目標檔，再按「儲存」，確認不覆蓋外部變更。另模擬寫入失敗，確認原檔未被截斷、表單修改仍在。 |
| **R6：設定留在本機。** 程式不讀取憑證、不執行設定檔中的命令，也不上傳設定。 | 在離線環境完成讀取、修改與儲存；確認只變更目標設定檔，不存取 `auth.json` 或專案設定。 |

## 可編輯欄位

以 Pi 1.6.0 的 61 個頂層 schema 鍵為分母。可設定及清除的鍵計一次；物件須涵蓋 schema 定義的所有子欄位才計入。覆蓋率由第一版的 3/61（4.9%）提高至 24/61（39.3%），共 32 個可編輯欄位。這些是 Pi 的偏好，不是 App 本身的設定。

| 群組 | JSON 鍵 |
| --- | --- |
| Models | `defaultProvider`、`defaultModel`、`defaultThinkingLevel`、`hideThinkingBlock`、`showCacheMissNotices` |
| Interaction | `steeringMode`、`followUpMode`、`doubleEscapeAction`、`treeFilterMode` |
| Appearance | `theme`、`quietStartup`、`tuiMode`、`fullscreenExitOutput`、`fullscreenScrollbar`、`fullscreenCopyOnSelect`、`fullscreenWheelScrollLines`、`editorPaddingX`、`outputPad`、`autocompleteMaxVisible`、`showHardwareCursor` |
| Appearance（巢狀） | `terminal.showImages`、`terminal.imageWidthCells`、`terminal.clearOnShrink`、`terminal.showTerminalProgress`、`terminal.hyperlinks`、`terminal.images`、`terminal.trueColor`、`images.autoResize`、`images.blockImages`、`markdown.codeBlockIndent`、`markdown.mermaid` |
| Updates | `collapseChangelog` |

- 若選單已有預設值，就在該選項加上 **(default)**，不另列預設選項。選取預設項目會清除明確設定。未編輯的既有設定即使等於預設值，也會保留。文字和數字欄位用 **Clear**；Markdown 縮排另可用 **Set empty** 設為空字串，不同於預設的兩個空格。未指定 provider 或 model 時，欄位顯示 **Default: auto-select**。
- 設定左側的短灰線會隨編輯即時更新。有效的欄位值與 Pi 1.6.0 內建預設不同時才顯示；明確指定 provider 或 model ID 也會標記，因為預設為自動選擇。比較不計入環境變數或專案設定。
- 數字欄位接受整數：`editorPaddingX` 為 0–3，`autocompleteMaxVisible` 為 3–20，`fullscreenWheelScrollLines` 為 1–100 或 `auto`，`terminal.imageWidthCells` 為 1–9007199254740991。不合法的編輯無法儲存。底部左側的狀態訊息會指出欄位與群組；按右側同列的 **Go to field** 可直接定位。不支援的既有值會保留。
- **Appearance** 依功能分段，內容超出視窗時顯示捲軸。Fullscreen 區段的設定可先編輯，但只會在 fullscreen 模式生效。
- 編輯巢狀欄位只會改變指定的鍵。儲存時保留同一物件內未編輯的值；清除最後一個子欄位時保留空物件。若原值不是物件，保留原值並停用該物件的欄位。
- Provider 和 model ID 可直接輸入；App 不會連線驗證。設定只寫入使用者檔案，不會立即套用至 Pi。專案可覆寫使用者設定，部分啟動設定須重新啟動 Pi。

## Pi `settings.json` 完整選項清單

### 研究版本與範圍

本節依本機安裝的 `@earendil-works/pi-coding-agent` **1.6.0**，核對上游 `v1.6.0` 的 TypeScript 原始碼，commit 為 `faac585884ae2c013a6b1400f3e9e77f10b34bac`。不是只依 `/settings` 選單列舉。此清單包含未提供編輯介面的鍵；App 的範圍以前一節為準。

研究依據：

1. [settings-schema.ts][pi-settings-schema]：接受的鍵、型別、巢狀物件與舊版欄位，共 61 個頂層鍵。
2. [settings-defaults.ts][pi-settings-defaults]：共用預設值。
3. [settings-manager.ts][pi-settings-manager]：實際讀取、使用者／專案合併、環境變數備援與舊格式轉換。
4. [sdk.ts][pi-sdk]、[package-manager.ts][pi-package-manager]、[main.ts][pi-main]、[config.ts][pi-config]、[package-manager-cli.ts][pi-package-cli] 與 [codemode/index.ts][pi-codemode]：設定的使用端與額外限制。
5. 同版本的 [Settings Reference][pi-settings-reference] 與 [Configuration][pi-configuration]：交叉核對使用方式。Pi 升級後須重新核對本清單。

讀表規則：

- 下表使用 `compaction.enabled` 這類路徑表示巢狀鍵；JSON 寫法是 `"compaction": { "enabled": true }`，不是名為 `"compaction.enabled"` 的頂層鍵。
- 所有頂層選項都可省略。預設欄表示未設定時的行為，不表示 Pi 或 App 必須把預設值寫入檔案。
- 「僅使用者層級」表示只讀取 `<agent-dir>/settings.json`，不接受專案 `.pi/settings.json` 覆寫。
- 專案設定通常在取得 project trust 後才載入；`sessionDir` 是例外，Pi 會先讀它以尋找 session。一般純量由專案覆寫，巢狀物件遞迴合併；資源清單與 `defaultTools` 另有下述規則。
- CLI、SDK 或恢復中的 session 仍可能決定實際值。修改後可用 `/reload` 重新讀取，但啟動選項不保證即時套用；須按該功能的使用時機驗證，必要時重新啟動 Pi。

### 模型、thinking 與 prompt cache

| JSON 路徑 | 型別／可用值 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `defaultProvider` | `string` | 自動選擇 | 啟動時的 AI provider。不是 API key。 |
| `defaultModel` | `string` | 自動選擇 | 啟動時的 model ID，搭配 provider 使用。 |
| `defaultThinkingLevel` | `"off"`、`"minimal"`、`"low"`、`"medium"`、`"high"`、`"xhigh"`、`"max"` | `"medium"` | 啟動時的 thinking level；實際支援程度依模型而定。 |
| `modelThinkingLevels` | `Record<string, thinking level>` | 無個別覆寫 | 鍵必須是完整的 `provider/modelId`；為該模型指定啟動 thinking level。 |
| `thinkingBudgets.minimal` | `number` | provider 內建預算 | 自訂 `minimal` 的 thinking token 預算。 |
| `thinkingBudgets.low` | `number` | provider 內建預算 | 自訂 `low` 的 thinking token 預算。 |
| `thinkingBudgets.medium` | `number` | provider 內建預算 | 自訂 `medium` 的 thinking token 預算。 |
| `thinkingBudgets.high` | `number` | provider 內建預算 | 自訂 `high` 的 thinking token 預算；沒有 `xhigh` 或 `max` 預算鍵。 |
| `enabledModels` | `string[]` | 所有可用模型 | 限定啟動選擇與模型切換範圍；pattern 格式同 `--models`。 |
| `hideThinkingBlock` | `boolean` | `false` | 在 transcript 隱藏 thinking，不是停用模型的 thinking。 |
| `showCacheMissNotices` | `boolean` | `false` | 顯示重大 cache miss、cache warming、compaction 用量與 provider 恢復通知。 |
| `cacheWarming` | `"off"`、`"streaming"`、`"idle"` | `"idle"` | **僅使用者層級。** 保持符合條件的 provider prompt cache；`streaming` 只在執行中維持，`idle` 也可在執行間維持最多 30 分鐘。刷新會產生用量與費用。 |

### 互動與 project trust

| JSON 路徑 | 型別／可用值 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `steeringMode` | `"all"`、`"one-at-a-time"` | `"one-at-a-time"` | 執行中排入的 steering 訊息，一次送出全部或逐則送出。 |
| `followUpMode` | `"all"`、`"one-at-a-time"` | `"one-at-a-time"` | follow-up 訊息的送出方式。 |
| `externalEditor` | `string` | `VISUAL` → `EDITOR` → Windows 的 `notepad`／其他平台的 `nano` | 外部編輯器命令；空白字串視為未指定。 |
| `doubleEscapeAction` | `"tree"`、`"fork"`、`"none"` | `"tree"` | 輸入框空白時連按兩次 Escape 的動作。 |
| `treeFilterMode` | `"default"`、`"no-tools"`、`"user-only"`、`"labeled-only"`、`"all"` | `"default"` | 開啟 `/tree` 時的初始篩選方式。 |
| `defaultProjectTrust` | `"ask"`、`"always"`、`"never"` | `"ask"` | **僅使用者層級。** 沒有既存 trust 決定時的處理方式；`always` 會允許載入專案資源，不能當成單純外觀設定。 |

### 工具與 codemode

| JSON 路徑 | 型別／可用值 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `defaultTools` | `string[]` | `["read", "bash", "edit", "write"]` | 啟動工具選擇。一般名稱取代繼承清單，`+name` 新增，`-name` 移除；沒有繼承清單時，空陣列停用所有 built-in tools，但不會停用 extension 或 SDK tools。 |
| `codemode.mode` | `"on"`、`"only"` | `"on"` | codemode 啟用時的工具呈現方式。`on` 保留直接宣告的工具；`only` 將可經 script 呼叫的工具集中到 codemode，隱藏直接工具宣告。此鍵本身不會啟用 codemode。 |
| `codemode.inlineBudget` | 非負 `number` | `3000` | codemode 工具描述可用於內嵌工具宣告的估計 token 數，以字元數除以 4 估算；放不下的工具可用 `searchTools()` 找到。`0` 只列 namespace。 |

- Built-in tools 包含 `read`、`bash`、`powershell`、`edit`、`write`、`grep`、`find`、`ls`。`defaultTools` 也能指定 extension 註冊的工具，例如預設為 inactive 的 `codemode`、`tool_search`。
- `defaultTools` 若只含 `+name`／`-name`，就在繼承清單上修改；若含一般名稱，先以一般名稱建立清單，再依序套用增減。專案層級也使用這個規則。1.6.0 的實作把專案 `[]` 視為沒有增減，因此不會清空已存在的使用者工具清單；移除繼承工具須使用 `-name`。
- `/reload` 會啟用新加入 `defaultTools` 的工具，但不會因工具被移除而停用它，也不會重新啟用使用者已關閉且設定未變的工具。CLI 工具選項仍可覆寫設定。

### Session、compaction 與 branch summary

| JSON 路徑 | 型別／可用值 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `sessionDir` | `string` | agent 目錄下的 `sessions/`，依工作目錄區分 | session 儲存目錄；相對路徑以工作目錄為準。`PI_CODING_AGENT_SESSION_DIR` 與 `--session-dir` 可覆寫。 |
| `sessionShareDestination` | `"auto"`、`"gist"`、`"radius"` | `"auto"` | `/share` 的目的地。`auto` 有 Radius 登入時使用 Radius，否則建立 GitHub gist；`radius` 登入失敗時不會改用 gist。 |
| `compaction.enabled` | `boolean` | `true` | 啟用自動壓縮 context。 |
| `compaction.reserveTokens` | 非負安全整數 | `16384` | context 可用 token 低於此保留量時觸發自動 compaction。 |
| `compaction.keepRecentTokens` | 非負安全整數 | `20000` | 不經摘要而保留的近期 token 數。 |
| `compaction.modelOverrides` | `Record<string, object>` | 無個別覆寫 | 鍵為完整的 `provider/modelId`，值可包含下列兩個欄位。 |
| `compaction.modelOverrides.<provider/modelId>.reserveTokens` | 非負安全整數 | 繼承 `compaction.reserveTokens` | 個別模型的保留量。 |
| `compaction.modelOverrides.<provider/modelId>.keepRecentTokens` | 非負安全整數 | 繼承 `compaction.keepRecentTokens` | 個別模型的近期保留量。 |
| `branchSummary.reserveTokens` | `number` | `16384` | branch history 摘要時，為 prompt 與模型回應保留的 token 數。 |
| `branchSummary.skipPrompt` | `boolean` | `false` | 不顯示是否摘要的提示，預設不產生摘要；不是自動同意摘要。 |

Compaction 的安全整數範圍是 `0` 至 `9007199254740991`。兩個 token 欄位各自按「模型覆寫 → 一般 compaction 設定 → 內建預設」解析；使用者與專案物件先合併，再找模型鍵。無效的 compaction token 值會在讀取時拋錯。

### Terminal、外觀與圖片

| JSON 路徑 | 型別／可用值 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `theme` | `string` | `"system"` | Pi 的主題名稱；內建有 `system`、`dark`、`light`，也可使用自訂名稱或 `lightTheme/darkTheme` 隨 terminal 明暗切換。不是 App 主題。 |
| `quietStartup` | `boolean` 或 `"header"` | `false` | `true` 隱藏啟動資訊；`header` 保留版本與快捷鍵標頭，但隱藏 model scope 與資源清單。 |
| `tuiMode` | `"regular"`、`"fullscreen"` | `"fullscreen"` | 互動式 terminal UI 模式。 |
| `fullscreenExitOutput` | `"transcript"`、`"resume-hint"` | `"transcript"` | 離開 fullscreen 時輸出 transcript 或恢復提示；regular 模式不使用。 |
| `fullscreenScrollbar` | `"auto"`、`"always"`、`"hidden"` | `"auto"` | fullscreen transcript 的 scrollbar；regular 模式不使用。 |
| `fullscreenCopyOnSelect` | `boolean` | `true` | fullscreen 選取文字後自動複製；regular 模式不使用。 |
| `fullscreenWheelScrollLines` | `"auto"` 或 `number` | `"auto"` | fullscreen 每次 wheel event 捲動行數；數字會取整並限制在 1–100。`auto` 依平台與 SSH 狀態調整，Alt+wheel 為五倍距離。 |
| `editorPaddingX` | `number` | `0` | 輸入框左右 padding，介面 setter 取整並限制在 0–3 cells。 |
| `outputPad` | `0`、`1` | `1` | transcript 左右 padding，包含訊息、工具輸出、`!` 命令輸出與摘要。 |
| `autocompleteMaxVisible` | `number` | `5` | autocomplete 顯示筆數，介面 setter 取整並限制在 3–20。 |
| `showHardwareCursor` | `boolean` | `false`，或由 `PI_HARDWARE_CURSOR=1` 啟用 | 使用 terminal cursor，而非 Pi 繪製的 cursor；明確設定優先於環境變數。 |
| `terminal.showImages` | `boolean` | `true` | terminal 支援時顯示 inline images；不控制送給模型的圖片。 |
| `terminal.imageWidthCells` | `number` | `60` | inline image 偏好寬度，單位為 cell；有限數值取整且至少為 1。 |
| `terminal.clearOnShrink` | `boolean` | `false`，或由 `PI_CLEAR_ON_SHRINK=1` 啟用 | 內容縮小時清除空白列；明確設定優先於環境變數。 |
| `terminal.showTerminalProgress` | `boolean` | `false` | 透過 OSC 9;4 在 terminal tab 顯示進度。 |
| `terminal.hyperlinks` | `boolean` 或 `"auto"` | `"auto"` | 覆寫 OSC 8 hyperlink 支援偵測。 |
| `terminal.images` | `"kitty"`、`"iterm2"`、`"auto"`、`false` | `"auto"` | 指定 inline image protocol 或停用；自動模式交由 terminal capability detection 決定。 |
| `terminal.trueColor` | `boolean` 或 `"auto"` | `"auto"` | 覆寫 true-color 支援偵測。 |
| `images.autoResize` | `boolean` | `true` | 送給模型前將圖片縮至最多 2000 × 2000 pixels。 |
| `images.blockImages` | `boolean` | `false` | 阻止圖片送給模型，與 terminal 是否顯示圖片分開。 |
| `markdown.codeBlockIndent` | `string` | `"  "`（兩個空格） | terminal Markdown code block 的縮排前綴。 |
| `markdown.mermaid` | `"off"`、`"final"`、`"streaming"` | `"streaming"` | Mermaid rendering 模式。 |

[theme.ts][pi-theme-runtime] 額外支援 `lightTheme/darkTheme` 語法，左右兩邊須為非空主題名稱，且只能有一個 `/`。它不是檔案路徑；格式錯誤時回到 system theme。

### 網路、timeout 與 retry

| JSON 路徑 | 型別／可用值 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `transport` | `"auto"`、`"sse"`、`"websocket"`、`"websocket-cached"` | `"auto"` | 支援多種 transport 的 AI provider 所用傳輸方式；不表示每個 provider 都支援所有值。 |
| `httpProxy` | `string` | 不額外指定 proxy | **僅使用者層級。** proxy URL，用於 Pi 管理的 HTTP clients；只補上尚未設定的 `HTTP_PROXY`、`HTTPS_PROXY`，不覆寫既有環境變數。 |
| `httpIdleTimeoutMs` | 非負 `number` 或 `"disabled"` | `300000` | HTTP header／body 閒置 timeout，單位 ms；`0` 或 `disabled` 停用。 |
| `websocketConnectTimeoutMs` | 非負 `number` 或 `"disabled"` | provider 預設；OpenAI Codex 為 `15000` | WebSocket 連線／handshake timeout，單位 ms；`0` 或 `disabled` 停用。 |
| `retry.enabled` | `boolean` | `true` | 啟用 agent 層級的暫時性錯誤重試。 |
| `retry.maxRetries` | `number` | `3` | agent 層級重試次數。 |
| `retry.baseDelayMs` | `number` | `2000` | 指數退避的起始間隔，單位 ms。 |
| `retry.maxAgentDelayMs` | `number` | `60000` | agent 層級重試間隔上限，單位 ms。 |
| `retry.provider.timeoutMs` | `number` | 跟隨 `httpIdleTimeoutMs` | provider request timeout，單位 ms，不接受字串 `disabled`。HTTP timeout 停用且此鍵未指定時，SDK 使用 `2147483647` ms。 |
| `retry.provider.maxRetries` | `number` | `0`（provider 層級預設） | provider 層級重試次數；通常保持 0，避免延遲 Pi 處理 quota 或 usage-limit 錯誤。 |
| `retry.provider.maxRetryDelayMs` | `number` | `60000` | server 要求的 retry delay 上限，單位 ms；`0` 停用上限。 |

### Shell 與更新命令

| JSON 路徑 | 型別／可用值 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `shellPath` | `string` | 平台預設 shell | 自訂 shell executable，可用 `~` 開頭；Windows 也可指定 Git Bash 或 Cygwin 路徑。 |
| `shellCommandPrefix` | `string` | 無前綴 | 加在每個 bash 命令前的內容，例如初始化 shell aliases；會影響命令執行。 |
| `npmCommand` | `string[]` | `["npm"]` | npm 查詢與安裝所用的 executable 和 arguments，使用 argv 陣列，不是整段 shell 字串；Pi 自身更新只採用使用者層級的 `npmCommand`。 |
| `selfUpdateCommand` | 非空 `string[]` | 依安裝方式判定更新方式 | **僅使用者層級。** `pi update` 使用的 argv，可含 `{packageName}`、`{version}`、`{installSpec}` placeholder；`PI_SELF_UPDATE_COMMAND` 優先。 |

### Packages 與資源探索

| JSON 路徑 | 型別／可用值 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `packages` | `(string 或 package object)[]` | `[]` | npm、git、URL 或本機 Pi package 來源。字串形式載入該 package 允許的全部資源；物件形式可篩選資源。 |
| `packages[].source` | `string`，物件形式必填 | 無 | package 來源，例如 `npm:@example/pi-tools@1.0.0`、`git:github.com/example/pi-tools@v1`、本機路徑。 |
| `packages[].autoload` | `boolean` | 自動載入允許的資源 | `false` 從空清單開始，只套用明確的資源 pattern；專案同來源項目可用它調整使用者 package，而非整份取代。 |
| `packages[].extensions` | `string[]` | 載入 package 允許的全部 extensions | 篩選 package 內的 extension；`[]` 表示不載入此類資源。 |
| `packages[].skills` | `string[]` | 載入 package 允許的全部 skills | 篩選 package 內的 skill；`[]` 表示不載入此類資源。 |
| `packages[].prompts` | `string[]` | 載入 package 允許的全部 prompts | 篩選 package 內的 prompt template；`[]` 表示不載入此類資源。 |
| `packages[].themes` | `string[]` | 載入 package 允許的全部 themes | 篩選 package 內的 theme；`[]` 表示不載入此類資源。 |
| `extensions` | `string[]` | `[]`，仍有預設探索與 built-in extensions | 額外 extension 檔案或目錄，也可控制 built-in extensions。 |
| `skills` | `string[]` | `[]`，仍有預設探索 | 額外 skill 檔案或目錄；不要再寫舊版 object 格式。 |
| `prompts` | `string[]` | `[]`，仍有預設探索 | 額外 prompt-template 檔案或目錄。 |
| `themes` | `string[]` | `[]`，仍有預設探索 | 額外 theme 檔案或目錄；與目前選取的 `theme` 分開。 |
| `enableSkillCommands` | `boolean` | `true` | 將 skills 註冊成 `/skill:name` 命令；不是控制是否探索 skills。 |

- 使用者資源相對路徑以 agent 目錄為準；專案資源以專案 `.pi/` 為準。也支援絕對路徑與 `~`。
- 資源清單支援 `!pattern` 排除 glob、`+path` 精確加入、`-path` 精確排除；使用者與專案的資源會一起載入，不能把所有陣列都當成「專案整份取代」。
- 同一 package 的身分按 npm package name、去除 ref 的 git repository URL 或本機絕對路徑判定。一般由專案宣告優先；`autoload: false` 的專案項目可作為使用者宣告的篩選差異。Package filters 不能載入 package manifest 未允許的資源。
- `extensions` 可指定 `builtin:mcp`、`builtin:llama.cpp`、`builtin:codemode`、`builtin:tool-search`；它們預設載入。`-builtin:<name>` 停用指定項目，專案的 `+builtin:<name>`／`-builtin:<name>` 優先於使用者設定。載入 extension 不代表它提供的所有工具都已啟用。
- App 只保留這些資源設定，不安裝 packages、不載入 extensions，也不執行其中的程式碼或命令。

### 更新資訊、telemetry 與警告

| JSON 路徑 | 型別／可用值 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `collapseChangelog` | `boolean` | `false` | 更新後顯示精簡 changelog；extension package 的 changelog 也會縮為每個 package 一行。 |
| `enableInstallTelemetry` | `boolean` | `true` | 匿名安裝／更新回報及部分 provider attribution headers；不控制更新檢查。依 [telemetry.ts][pi-telemetry]，`PI_TELEMETRY` 優先；設為 `1`、`true`、`yes` 才啟用，後兩者不分大小寫。 |
| `enableAnalytics` | `boolean` | `false` | 選擇加入 analytics 資料分享；1.6.0 的設定參考標示目前僅用於實驗性首次設定流程。 |
| `warnings.anthropicExtraUsage` | `boolean` | `true` | 使用 Anthropic 訂閱登入時，警告可能產生付費 extra usage。 |

### Schema 與 Pi 自動維護的狀態

這些鍵也能出現在 `settings.json`，但不是一般偏好。App 應保留，不應當成可任意重設的表單欄位。

| JSON 路徑 | 型別 | 未設定時 | 用途與限制 |
| --- | --- | --- | --- |
| `$schema` | `string` | 無 | 編輯器 validation 與 completion；官方值為 `https://pi.dev/schemas/settings.schema.json`。此網址跟隨發布版本，不是固定的 1.6.0 schema。 |
| `lastChangelogVersion` | `string` | 尚未記錄 | Pi 最近已顯示的 changelog 版本。 |
| `extensionChangelogVersions` | `Record<string, string>` | 尚未記錄 | 依 extension package key 記錄已顯示的 changelog 版本。 |
| `trackingId` | `string` | 尚未產生 | analytics identifier；透過 Pi 的 setter 首次啟用 analytics 時產生 UUID。 |
| `deviceId` | `string` | 首次需要時產生 | 安裝實例的穩定 UUID，例如供 authentication／provider identity 使用；只採用使用者層級，不接受專案共用同一 ID。 |

### 舊版相容欄位

Pi 的 `migrateSettings()` 在讀入時轉換以下格式；單純載入不會立即改寫檔案，後續儲存才可能持久化轉換。新檔應使用現行鍵；本 App 不應為了編輯其他欄位而主動改寫舊欄位。

| 舊 JSON 路徑 | 型別／可用值 | 轉換方式 |
| --- | --- | --- |
| `queueMode` | `"all"`、`"one-at-a-time"` | 未指定 `steeringMode` 時轉成 `steeringMode`。 |
| `websockets` | `boolean` | 未指定 `transport` 時，`true` 轉成 `"websocket"`，`false` 轉成 `"sse"`。 |
| `skills.enableSkillCommands` | `boolean`，舊版 `skills` object 內 | 未指定頂層 `enableSkillCommands` 時移到該鍵。 |
| `skills.customDirectories` | `string[]`，舊版 `skills` object 內 | 非空陣列轉成頂層 `skills` 陣列；否則移除舊版 `skills` object。 |
| `retry.maxDelayMs` | `number` | `retry.provider.maxRetryDelayMs` 缺少或為 `null` 時補到該欄位，然後移除舊鍵。 |

另有不在現行 schema 內的舊版 `apiKeys`（provider 名稱對字串的 object）。[migrations.ts][pi-migrations] 的 `migrateAuthToAuthJson()` 在使用者目錄沒有 `auth.json` 時，嘗試搬移字串值至 `auth.json` 並移除舊鍵；已有 `auth.json` 則略過。這是 credentials migration，不是現行設定選項；App 不將其中的憑證解析為表單欄位、不執行遷移，儲存其他欄位時仍保留原值。

### 邊界與驗證注意事項

- `SettingsSchema` 允許額外的頂層鍵；Pi 的檔案載入不是對整份檔案執行嚴格 schema validation。能保存任意鍵不等於 Pi 核心會使用它。本清單只涵蓋此版本核心與內建 codemode 所定義的欄位；第三方 extensions 的自訂鍵須另查其原始碼，App 仍須保留。
- 表格的型別來自 schema，部分 runtime parser 另有容錯。例如 HTTP／WebSocket timeout 也接受可解析的數字字串；新檔仍應寫 schema 所列型別。`editorPaddingX` 與 `autocompleteMaxVisible` 的範圍由 setter 限制，不能宣稱手動 JSON 一定有同等 validation。
- API keys 與 OAuth credentials 屬於 `auth.json`；自訂 provider endpoints／模型定義屬於 `models.json`；MCP server 宣告屬於 `mcp.json`；快捷鍵屬於 `keybindings.json`。它們不是本清單中可搬到 `settings.json` 的選項。

[pi-settings-schema]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/core/settings-schema.ts
[pi-settings-defaults]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/core/settings-defaults.ts
[pi-settings-manager]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/core/settings-manager.ts
[pi-sdk]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/core/sdk.ts
[pi-package-manager]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/core/package-manager.ts
[pi-main]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/main.ts
[pi-config]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/config.ts
[pi-package-cli]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/package-manager-cli.ts
[pi-codemode]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/extensions/codemode/index.ts
[pi-settings-reference]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/docs/settings.md
[pi-configuration]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/docs/configuration.md
[pi-migrations]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/migrations.ts
[pi-theme-runtime]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/modes/interactive/theme/theme.ts
[pi-telemetry]: https://github.com/earendil-works/pi/blob/faac585884ae2c013a6b1400f3e9e77f10b34bac/packages/coding-agent/src/core/telemetry.ts

## 成功標準

- 使用者不開文字編輯器，就能找到生效的使用者設定檔、改一個欄位、儲存並重新開啟 App 確認結果。在沒有專案覆寫的測試環境中，Pi 也能讀到這項變更。
- 在暫存目錄通過上述驗收案例，尤其是保留未編輯設定、讀取失敗時不覆蓋，以及外部變更後拒絕儲存。這些測試不使用真實的 `~/.pi/agent/`。

## 依賴與待確認事項

- **Pi 相容性：** [Pi Configuration](https://docs.pi.dev/latest/configuration) 定義 agent 目錄、使用者設定檔與 `/reload`。App 只顯示檔案裡的使用者設定，不計算與專案設定合併後的結果。
- **Rune 版本：** Rune 仍為預發布版本，依上游指引固定 Git commit。已用 Rune 的 headless 模式驗證表單，並在 Linux 桌面驗證視窗和鍵盤操作。
- **首發平台：** Linux。macOS、Windows 和安裝檔的發佈範圍待確認。
- **符號連結：** 第一版顯示說明並拒絕讀寫指向其他檔案的 `settings.json` 符號連結，不取代連結或改寫其目標。
