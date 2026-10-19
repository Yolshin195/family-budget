+++
title = "Past figures no longer change after the fact"
date = 2026-10-19

[taxonomies]
topics = ["planning", "accounts", "currencies"]

[extra]
change = "fixed"
+++
- A new start date or repeat on a plan that already had payments no longer loses past payments or creates "overdue" payments in the past: the old plan is closed and the new rule applies from today.
- Only a real payment of a plan can be marked as paid: not skipped, on a date of its rule and of the same type — before, the API accepted the "wrong" date and the forecast counted the payment twice.
- A category's type can't be changed once it has transactions, plans or limits, so expenses don't turn into income.
- A currency has 0 to 4 decimal places, which can't be reduced once it has amounts; the base currency and a currency with active accounts can't be archived.
- Clicking "Save" twice in the account form no longer records the account's "Balance" twice.
