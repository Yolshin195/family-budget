+++
title = "An amount without a rate is no longer added to baht 1:1"
date = 2026-10-19

[taxonomies]
topics = ["currencies"]

[extra]
change = "fixed"
+++
If a currency has no exchange rate, its amounts are left out of the Overview and Calendar totals — before, €10 counted as ฿10. Now the category shows a "no rate: 10 €" tag, and the transaction form, quick entry and Claude warn you while you type: "No EUR rate to the base currency — the amount won't count in the budget until you add a rate".
