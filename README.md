# Osu! Beatmap Seekman

Tauri 2 + React + Rust 妗岄潰绋嬪簭锛岀敤浜庢寜鏉′欢鎼滅储 osu! ranked/loved beatmapset锛屽苟鎵归噺涓嬭浇鍒?osu! 鐨?`Songs` 鏂囦欢澶广€?
## 鍚姩

```powershell
npm install
npm run dev
```

涔熷彲浠ョ洿鎺ヨ繍琛岋細

```powershell
.\run.ps1
```

鎴栧弻鍑?`run.bat`銆傝剼鏈細浼樺厛鎵撳紑宸叉瀯寤虹殑 Tauri exe锛涘鏋?exe 涓嶅瓨鍦ㄦ垨婧愮爜鏇存柊锛屽垯鑷姩閲嶆柊鏋勫缓銆?
## osu! API 濉粈涔?
杩涘叆 osu! 缃戦〉绔处鍙疯缃紝鍒涘缓涓€涓?OAuth Application锛?
- `Client ID`锛歰su! 缁欎綘鐨勫簲鐢ㄦ暟瀛?ID銆?- `Client Secret`锛歰su! 缁欎綘鐨勫簲鐢ㄥ瘑閽ャ€?- `Bearer Token`锛氫竴鑸暀绌恒€?
鎼滅储 beatmapset 鍙渶瑕?`Client ID + Client Secret`锛岀▼搴忎細鑷姩鐢?`client_credentials` 鑾峰彇 `public` token銆備笅杞界幇鍦ㄨ蛋闀滃儚婧愶紝涓嶅啀渚濊禆 osu! 瀹樻柟涓嬭浇鎺ュ彛銆?
## 宸插疄鐜?
- 閫夋嫨 osu! `Songs` 鏂囦欢澶逛綔涓轰笅杞界洰鏍囥€?- 鎵弿鏈湴宸叉湁 beatmapset锛岄伩鍏嶉噸澶嶅姞鍏ヤ笅杞姐€?- 鏀寔 Ranked / Loved銆佹棩鏈熸銆佹槦鏁般€丱D銆丠P銆丆S銆丄R銆丅PM銆侀暱搴︺€佹ā寮忋€乵ania 4K/7K銆佸叧閿瘝绛涢€夈€?- 鏃ユ湡浼氫綔涓?osu! 鎼滅储璇硶浼犲叆锛屼緥濡?`ranked>=2024-01-01 ranked<=2024-12-31`锛屽悓鏃朵繚鐣欐湰鍦颁簩娆℃牎楠屻€?- 鏀寔鎸夋椂闂淬€佹椂闀裤€丅PM 姝ｅ簭/鍊掑簭鎺掑簭銆?- 涓嬭浇鍙€夋嫨甯﹁棰戞垨涓嶅甫瑙嗛銆?- 涓嬭浇浣跨敤 beatmapset id 浠庨暅鍍忕珯鑾峰彇 `.osz`锛屾敮鎸?Sayobot銆丠inamizawa銆丆atboy銆丯erinyan銆?- 鐢ㄦ埛鍙湪渚ф爮璋冩暣闀滃儚浼樺厛绾э紱涓嬭浇澶辫触鎴栬姹傝秴鏃朵細鑷姩鍒囨崲涓嬩竴涓暅鍍忋€?- 涓嬭浇鏃跺疄鏃舵樉绀哄凡涓嬭浇 MB/GB銆?- 浣跨敤 `.osz.part` 鏂囦欢鏀寔鏂偣缁紶銆?- 浠诲姟鍜岃缃繚瀛樺埌 Tauri 搴旂敤鏁版嵁鐩綍鐨?`state.json`銆?
## 鏋勫缓

```powershell
npm run tauri:build
```

debug 楠岃瘉浜х墿绀轰緥锛?
```text
src-tauri/target/debug/osu_beatmap_seekman.exe
src-tauri/target/debug/bundle/nsis/Osu! Beatmap Seekman_1.0.0_x64-setup.exe
```
