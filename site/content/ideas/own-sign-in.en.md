+++
title = "Our own sign-in: one tap and in Thai"
description = "Sign-up and sign-in without passwords, in all three languages and into all our apps at once."
date = 2026-10-17

[taxonomies]
topics = ["security"]

[extra]
status = "planned"
+++

Today sign-up and sign-in are handled by an off-the-shelf server, Rauthy. It is reliable, but it has limitations that get in our users' way:

- the sign-in pages and code emails exist only in Russian and English — **there is no Thai**;
- there are no authenticator app codes (TOTP), only codes by email;
- after signing out it doesn't always bring you back to the site.

So we plan our own sign-in service. What we want:

- **one-tap sign-in** — passkey, Telegram, Google, GitHub; email is optional;
- **three languages** on every page and in every email;
- **one sign-in for all our apps**: signed in to one — already signed in to the others, and "sign out everywhere" signs you out of all of them;
- **access for assistants** like Claude — under the same rules as now.

Accounts and data will be kept during the switch. We will announce the timing in "What's new".
