+++
title = "Time zone of a transaction"
description = "The time of a purchase on a trip, by the local clock: each transaction has its own zone, and each member has their own “today”."
date = 2026-10-25

[taxonomies]
topics = ["family", "accounts"]

[extra]
status = "planned"
+++

Transactions already have a time, but the app counts it by one clock — the server's time zone. If you're on a trip and buy a coffee in Moscow, it shows up in the feed a few hours earlier or later, and a late-evening purchase can land on another day.

## The idea: a zone for each transaction and each person

- **Transaction zone** — the time is shown as it was where you paid: “09:15 (UTC+3)” if the zone differs from yours.
- **Member zone** — the browser suggests it, and you can pick it in your profile. It decides “today” and the time of new transactions.

Reports by day and month stay by the transaction date, as now.

## What we're still deciding

- How to show transactions of members in different countries in a shared family feed.
- Whether transactions from bank SMS take the bank's zone or the phone's.

If you travel a lot, tell us how you'd like to see the time on the [Reviews](@/feedback.en.md) page.
