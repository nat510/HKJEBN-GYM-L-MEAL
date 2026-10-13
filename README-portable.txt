gymL - portable Windows app

Windows 10/11 x64. Extract the ZIP into a writable folder and run gymL.exe.
Keep gymL_data beside the EXE when moving the app. No installation is required.

The app fetches products, coupons and promotions directly from the retailer
each time it starts. Use F5 or Update to fetch again. Cached data stays visible
during fetching; failed or incomplete fetches preserve the last successful
data and display a warning. The capture time is shown with the data.

Current prices and source-published effective-date price changes are included.
The app does not record or display price history.

Use categories, search, promotion filters and sorting to browse products.
The expensive and excluded-name filters are hidden by default. The latest-change
filter shows the latest source batch only when its date can be reliably matched.
Dates without a year stay yearless. Prices are in HKD and the retailer is the
authority for current prices.

Double-click an item, press Enter, or choose View image to open its image.
The native image viewer has fit and 1:1 modes with scrollbars, and previous/next
buttons. Coupon and promotion hidden-name keywords share a saved editable list.
Ctrl+F focuses search. The app repaints after activation/restoration.

After each launch or manual refresh finishes, fetched caches not successfully
refreshed for 30 days are removed. Newly confirmed data is kept even if unchanged.
Preferences are never removed by this cleanup. Images load only when viewed and
have a 32 MB disk-cache budget; a single decoded image has a 64 MB limit. No
background polling or continuous painting runs while idle.

gymL_data contains catalog.json, coupons.json, promotions.json,
preferences.json and an images folder. Anonymous retailer session cookies
remain in memory only and are discarded after fetching.

To build from Rust source, run scripts/build-portable.ps1 in PowerShell.
The build uses static CRT, LTO, stripped symbols and anonymous build paths.
The supplied icon is embedded in the executable.

THIRD_PARTY_LICENSES.txt contains dependency notices.
