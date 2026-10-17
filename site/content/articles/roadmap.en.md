+++
title = "Roadmap: what's next for Family budget over the coming six months"
description = "Numbers you can trust, data export, exchange rates that update themselves, logging an expense with one message in Telegram, and \"mine / shared\" in the family — what's coming and in what order."
date = 2026-10-17T19:00:00

[taxonomies]
topics = ["family", "currencies", "claude"]
+++

A morning in Phuket. Coffee — 80 baht in cash. A taxi — paid with a Thai card. In the afternoon, rubles become baht at an exchange office, at a rate a couple of percent off the market. In the evening, the rent invoice arrives in dollars. Logging each of these expenses on its own is easy. The hard part is knowing how much the family has really spent this month — and how much is still left.

We build Family budget for exactly these families: their money lives in two or three currencies, and the people they love read in different languages. In this article we share what's coming to the app over the next six months, in what order and why — and what we deliberately won't do.

## What already works

The app is at version 0.1 today, and it is already a complete budget:

- **A family with roles.** A shared project and personal ones, the "Owner", "Editor" and "Viewer" roles, invitations by link or QR code.
- **An expense in 5 seconds** from any page: amount, category, Enter.
- **Currency exchange as one transaction** — at the exchange office's real rate and with its fee, not at an "approximate" one.
- **Limits that look ahead:** how much will be left by the end of the month is visible before you save — with planned payments taken into account.
- **Claude keeps the budget for you:** expenses by voice or from a photo of a receipt, right from the chat.
- **Three languages, down to the data:** a transaction description has translations, and anything untranslated is marked.

Every feature is explained in detail in the [guides](@/guides/_index.en.md).

## Roadmap

Six stages. The timing is approximate: the order may change if you ask for something more important. We will keep this article up to date: when a stage ships, its status changes to "Done", and the details appear in ["What's new"](@/releases/_index.en.md).

<div class="roadmap">

### Numbers you can trust

{{ <stage status="in_progress" when="October — November 2026" lang="en" /> }}

A budget is only useful if you can trust it. So the first thing is not new buttons but accuracy. We have rechecked every calculation and will fix the places where the numbers could drift away from real life:

- the yearly budget will count monthly limits precisely, month by month;
- if a currency has no rate, the amount will be marked as having no rate instead of being added to the others one to one;
- editing a regular payment will no longer touch months that are already paid;
- the tree of categories and limits will get its indents back.

At the same time we are strengthening account protection and making updates more reliable — invisible work, but the most important.

### Your data is yours

{{ <stage status="planned" when="November 2026" lang="en" /> }}

Trying a new app is not scary when it's easy to leave. That's why:

- **CSV export** — transactions, accounts and reference data in one file, at any time; it opens in Excel and Google Sheets;
- **an open-source license** — the source code is already public on GitHub, and with a license you can legally run it on your own server;
- **contact without GitHub** — you will be able to write to us directly ([idea "Reviews and comments right on the site"](@/ideas/feedback-on-site.en.md)).

### Comfortable every day

{{ <stage status="planned" when="December 2026" lang="en" /> }}

The small things that turn into a habit:

- **exchange rates update themselves** every day; you can still enter your own rate in "Exchange rates", and it wins;
- **hints and errors in forms — in your language**, not only in Russian;
- **"today" follows your clock:** a time zone in your profile, so an expense at midnight lands on the right day;
- **search in transactions** — by text and amount, with filters by category and period;
- **a quick start:** on first sign-in the app suggests your family's currencies, accounts and regular payments — the budget is ready in a couple of minutes;
- **the Buddhist calendar** in the Thai interface.

### An expense in one message in Telegram

{{ <stage status="planned" when="December 2026 — January 2027" lang="en" /> }}

The main reason people give up on budgeting is manual entry. For those who use Claude, it's no longer in the way. Now it's for everyone: a Telegram bot that understands an ordinary message and lives in your family chat.

<figure class="chat">
<div class="chat-head">{{ <icon name="message-circle" cls="icon text-accent-text" /> }}Family · Telegram<span class="badge badge-accent">{{ <icon name="wallet" cls="icon icon-xs" /> }}Family budget</span></div>
<div class="chat-body">
<div class="msg msg-user"><span class="msg-who">Alexey</span>coffee 120 card SCB</div>
<div class="msg msg-ai"><span class="msg-who">Family budget</span>Expense 120&nbsp;฿ · Cafes &amp; restaurants · SCB. 6,380&nbsp;฿ left for Food this month.<span class="msg-actions"><span class="msg-btn">Undo</span></span></div>
<div class="msg msg-partner"><span class="msg-who">Anna</span>แท็กซี่ 80+45 เมื่อวาน</div>
<div class="msg msg-ai"><span class="msg-who">Family budget</span>รายจ่าย 125&nbsp;฿ · แท็กซี่ · เงินสด · เมื่อวาน<span class="msg-actions"><span class="msg-btn">เลิกทำ</span></span></div>
</div>
<figcaption class="chat-caption">Everyone writes in their own language — and the bot replies to each of them in theirs.</figcaption>
</figure>

What the bot will do:

- **log expenses from a message:** an amount with arithmetic ("80+45"), a category and an account by name in any of the three languages, "yesterday" and other dates;
- **work in the family chat:** an expense is recorded on behalf of whoever wrote it, and a "Viewer" only watches;
- **remind you:** rent is due tomorrow, the "Cafes & restaurants" limit is almost used up, and on Sundays — a weekly summary for the whole family;
- **undo:** an "Undo" button right under the message — just like in the app.

The same stage also brings:

- **moving from other apps:** import of CSV files and bank statements — KBank, SCB, T-Bank, Sber — with duplicate detection and ready-made settings for ZenMoney and CoinKeeper;
- **purchases in another currency:** you paid a Thai QR code from a Sber card — the transaction shows both the baht and the rubles charged.

### Family and money

{{ <stage status="discussing" when="February — April 2027" lang="en" /> }}

Once entry stops taking time, you'll want to see more. Here we especially look forward to your feedback — it decides what ships first:

- **charts:** spending by month, breakdown by category, the family's total wealth in one currency;
- **"mine / shared":** a personal account inside a family project is visible only to you;
- **who owes whom** — settling up between members;
- **savings goals, debts and credit cards;**
- **a receipt photo on a transaction** — for warranties and returns;
- **a phone without a connection:** the expense is saved and sent to the server once you're back online;
- **a LINE bot** that reads PromptPay slips — for those who live in LINE.

### Version 1.0

{{ <stage status="planned" when="April — May 2027" lang="en" /> }}

A version you can confidently recommend to friends:

- **our own sign-in:** one tap — with a passkey or through Telegram, no passwords, and in Thai too ([more](@/ideas/own-sign-in.en.md));
- **the site at the main address** — together with the guides, the blog and this roadmap;
- **ChatGPT and Gemini** in addition to Claude — if you need them ([tell us](@/ideas/more-assistants.en.md));
- **speed and security checks** against open standards — before we invite more people.

</div>

## What we won't do

Sometimes it's more honest to say "no" right away:

- **We won't connect to banks directly.** In Russia banks block third-party connections, Thai banks aren't open to them at all, and where syncing does exist, it's exactly what breaks most often. A message to the bot, a photo of a receipt or a statement file give the same speed and don't depend on anyone else's decisions.
- **No ads and no trackers.** Your spending is not a product.
- **An assistant won't be able to delete anything.** Claude and future bots only add and edit what you've allowed, and permissions are changed only in the browser.
- **We're in no hurry with separate phone apps.** The site already installs on your phone like an app; first it will learn to work offline.

## What stays the same

- **Three languages, always.** Every new feature ships in Russian, English and Thai at once.
- **Numbers over features.** Nothing new ships while there are known errors in the calculations.
- **Every change goes into "What's new"**, with an explanation of what changed and why.

## How to shape the plan

The roadmap is not set in stone. If something is missing for you, tell us:

- on the ["Reviews"](@/feedback.en.md) page or in [GitHub Issues](https://github.com/Yolshin195/family-budget/issues);
- in the ["Ideas"](@/ideas/_index.en.md) section — it lists what we are discussing and planning, with statuses;
- and follow each stage as it ships in ["What's new"](@/releases/_index.en.md).

We live between languages and currencies ourselves, and we build the app we want to use every day. Thank you for being with us.
