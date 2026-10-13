# gymL v1.0

Portable Windows 10/11 x64 application written in Rust with native Win32 controls.

- Run `gymL.exe` from a writable folder; no installation is required.
- Fetches products, coupons and promotions directly from the retailer at startup; F5 refreshes manually.
- Stores data in `gymL_data` beside the executable.
- Includes current prices, source-published effective-date changes, search, filtering, sorting and native image viewing.
- Removes fetched caches not successfully refreshed for 30 days after fetching completes; newly confirmed data and preferences remain.
- Repaints after activation and restoration from minimization.
- No recorded price history or scheduled fetching.

Extract the portable ZIP and run `gymL.exe`. Keep `gymL_data` beside it when moving the app. The ZIP includes usage instructions, dependency notices and an empty data folder.

Release assets: `gymL.exe`, `gymL-portable-windows-x64.zip`, `SHA256SUMS.txt` and `THIRD_PARTY_LICENSES.txt`.
