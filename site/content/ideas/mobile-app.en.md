+++
title = "An app for your phone and computer — that works offline"
description = "A dedicated app for Android, iPhone and computers: works offline and picks up transactions from bank notifications and receipt screenshots."
date = 2026-10-21

[taxonomies]
topics = ["accounts", "claude", "security"]

[extra]
status = "discussing"
+++

Today the budget lives in the browser and needs the internet. We want to build an app for Android, iPhone and computers that works without a connection and takes records from where they already appear: bank notifications and receipt screenshots.

## Without the internet

Transactions are saved right on the device. Once you are back online, the app sends them to the server and gets what other budget members have added. On a plane, at a market with no signal or abroad without roaming, you record expenses as usual.

## Transactions from notifications and the gallery

- **Android:** the app reads bank notifications and suggests a ready transaction — the amount, account and place are filled in, you only confirm. Screenshots of receipts and transfers in the gallery are recognised the same way.
- **iPhone:** Apple does not let any app read other apps' notifications. Instead there is the Share button for a screenshot and the Transaction automation in Shortcuts for Apple Pay.

The phone does the recognition itself. If a record is unclear, you can send it to Claude.

## Your budget in Telegram

Even before the app — a bot and a mini app in Telegram. Forward an SMS or a bank screenshot to the bot and it suggests a ready transaction; you only tap Save. The bot reminds you of planned payments and limits, and the menu button opens balances and the budget right inside Telegram, with no password.

## Claude on your computer without the server

In the computer app, Claude Desktop will be able to work with the budget directly, even offline. Permissions and the ban on deletion stay the same as today. On the phone, Claude still connects through the server.

## Security

Data on the device will be encrypted, and the app will open with your fingerprint or face. You turn on access to notifications and the gallery yourself and can turn it off at any time.

We will start with a trial version for Android. If you need such an app or know which notifications your bank sends, tell us on the [Reviews](@/feedback.en.md) page.
