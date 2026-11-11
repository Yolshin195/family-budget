+++
title = "Transactions: expenses, income and transfers"
description = "Quick entry in five seconds, a receipt with several items, transfers and currency exchange between accounts, finding and fixing transactions."
date = 2026-10-17T17:40:00

[taxonomies]
topics = ["start", "accounts"]

[extra]
level = "beginner"
+++

Money moves through the app in three kinds of transactions: an expense, income, and a transfer between your own accounts. You can log a transaction from any page, and in the "Transactions" section you can find, fix or delete it.

## 1. Quick expense

Click "Expense" in the menu, press <kbd>N</kbd>, or tap the round "+" on a phone. Enter the amount — you can do the maths right away: "120+45". Pick a category from the frequent ones or find it with search, which understands names in any of the three languages. If you have places under "Directories", search finds them too: pick 7-Eleven and the category and account are filled in (more in ["Places: shops, cafés and markets"](@/guides/places/index.en.md)). The account defaults to the one you paid from last time and the date to "Today"; you can change both. Enter saves and closes the window; "Save & add another" (<kbd>Ctrl</kbd> + <kbd>Enter</kbd>) keeps it open for the next expense.

![The "New expense" window: amount, frequent categories, account and date](quick.en.webp)

When a category has a limit, the window shows what's available and what will remain after this expense, and warns you at 80% and when you go over. You can still save — it's just a warning. Made a mistake? Click "Undo" in the pop-up message.

The same window has an "Expense", "Income", "Transfer" switch; the <kbd>I</kbd> key opens income straight away, and <kbd>T</kbd> a transfer. The <kbd>?</kbd> key shows all keyboard shortcuts.

## 2. A receipt with several items

Groceries, a coffee and pastries on one receipt? Click "Full details" in the window — the full form opens, and the amount you entered becomes the first item. Each item has its own category, amount and "Name", as on the receipt; "Add item" adds a row and the "Total" adds itself up. Tick "Refund" when a shop gives money back: the amounts reduce the categories' actuals.

The "× price" button opens "Quantity", "Unit" (pcs, kg, l, g), "Price" and "Discount" under the item, and the app works out the item amount: two croissants at 25 with a discount of 3 make 47, 0.45 kg of bananas at 60 makes 27. To type the amount yourself, clear the price. A discount on the whole receipt goes in "More", field "Receipt discount": it is split across the items in proportion to their amounts. “More” also has “For whom by default” and “Different recipients in one receipt” for purchases that aren't for everyone: see [“People and “For whom””](@/guides/people/index.en.md).

![Full details: a 7-Eleven receipt with prices and quantities](split.en.webp)

The "Description" is optional; write it in any of the three languages and set the "Original language". Translations into the other languages go in "Description translations", so everyone in the family reads the transaction in their own language. A saved transaction also has a "Postings" block showing how it's recorded in double entry; you don't need to touch it.

## 3. Transfers and currency exchange

A transfer between your own accounts isn't an expense: it doesn't change the budget or limits. Choose "Transfer", the "From" and "To" accounts and the "Debited" amount. If the accounts are in different currencies, a "Credited" field appears — what arrived in the second account's currency — and the "Rate" line shows the resulting rate next to the rate from your directory.

![A transfer from roubles to baht with a fee](transfer.en.webp)

Did the bank charge a fee? Open "Fee", choose a category and an amount. The fee is an expense: it counts in the budget and limits, and the rate is calculated without it. The quick window has an "Add a fee" link for this.

## 4. The transaction list

The "Transactions" section shows all of the month's transactions by day; the arrows switch months. At the top are the totals: income, expenses and net in each currency. Filters: account, type ("Expenses", "Income", "Transfers", "Opening balances"), member in a family budget, “For whom”, and the "Description not in my language" checkbox, which finds transactions that need a translation.

![A month's transactions with filters and totals](list.en.webp)

Within a day, transactions are ordered by time — it comes first in the line under the description: “08:40 · 7-Eleven · Sukhumvit 24 · KBank card · Алексей”. Change the time in the card next to “Date”. If you leave it empty, a transaction for today gets the current time, while one for another day stays without a time and is placed by when it was entered. Transactions from a bank SMS take the time from the SMS, so the order matches the bank's messages.

A receipt with several items is shown by category: the category total, and under it the goods in receipt order — "Croissant · 2 pcs × 25 − 3 → 47", or "Latte · 45" without a price. A transaction with a single unnamed item looks as usual. A label with a name — “Pinpan”, “For myself” — shows who the purchase is for; “For everyone” isn't labelled.

Transactions paid from a calendar plan carry a "planned" badge. A badge with a language code and a question mark, such as "EN?", means the description has no translation into your language.

## 5. Fix, translate, delete

Every transaction has "Translate description", "Edit" and "Delete" buttons. After deleting, you have a few seconds to click "Undo". A statement for a single account, with the balance after each transaction, opens from the "Accounts" section — see [Accounts, statements and reconciling balances](@/guides/accounts/index.en.md).
