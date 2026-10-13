# gymL v1.1

Portable Windows 10/11 x64 application written in Rust with native Win32 controls.

Changes in v1.1:

- Retailer requests now use the current Windows x64 Chrome User-Agent, verified on 2026-10-13:
  `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/156.0.0.0 Safari/537.36`
- Application source and executable are licensed under GNU AGPL version 3 or any later version (`AGPL-3.0-or-later`). The portable package includes `LICENSE`; dependency notices retain their original license terms.
- [Complete corresponding source](https://github.com/nat510/HKJEBN-GYM-L-MEAL/tree/v1.1) and [source ZIP](https://github.com/nat510/HKJEBN-GYM-L-MEAL/archive/refs/tags/v1.1.zip) include the build scripts.

- Run `gymL.exe` from a writable folder; no installation is required.
- Fetches products, coupons and promotions directly from the retailer at startup; F5 refreshes manually.
- Stores data in `gymL_data` beside the executable.
- Includes current prices, source-published effective-date changes, search, filtering, sorting and native image viewing.
- Removes fetched caches not successfully refreshed for 30 days after fetching completes; newly confirmed data and preferences remain.
- Repaints after activation and restoration from minimization.
- No recorded price history or scheduled fetching.

Extract the portable ZIP and run `gymL.exe`. Keep `gymL_data` beside it when moving the app. The ZIP includes usage instructions, dependency notices and an empty data folder.

Release assets: `gymL.exe`, `gymL-portable-windows-x64.zip`, `SHA256SUMS.txt`, `LICENSE` and `THIRD_PARTY_LICENSES.txt`.
