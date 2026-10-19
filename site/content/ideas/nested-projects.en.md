+++
title = "Nested projects: everyone sees only their part"
description = "Projects inside projects: the family total at the top, while a nanny or an accountant sees only their part of the budget."
date = 2026-10-19

[taxonomies]
topics = ["family", "security"]

[extra]
status = "planned"
+++

Today access is granted to a whole project: a member sees all its accounts and transactions — or nothing at all. That's why personal spending has to live in a separate project, as the [Projects & members](@/guides/family/index.en.md) guide suggests. But then the projects live apart: there is no family total, your own currencies, rates and categories have to be set up in each one again, and a transfer from one project to another has to be recorded twice.

## The idea: projects inside projects

A project will be able to contain other projects — sub-projects. Each sub-project has its own accounts, transactions, limits and planned payments, and the parent project shows the total of everything nested in it. Access can be given at any level: a person sees that project and everything inside it, but not what is above or beside it.

For example, this is how the budget of a family that lives in Thailand and rents out a flat in Moscow could look:

- **Family** — a summary of all parts; the owners are Aleksey and Anna
  - **Home** — rent, groceries, utilities
    - **Kids** — school, classes, the nanny; the nanny is an "Editor" only here
  - **Moscow flat** — rental income in rubles; the accountant is a "Viewer" only here
  - **Personal: Aleksey** — private, only Aleksey sees it
  - **Personal: Anna** — private, only Anna sees it

The nanny logs spending from the cash handed over and sees neither salaries nor savings. The accountant sees the rental income for the tax return — and nothing else. Personal spending stays personal, and the family total is on one screen.

## How permissions will work

- **A role works down the tree.** "Owner", "Editor" or "Viewer" in a project is the same role in all its sub-projects. In a sub-project a role can be raised, but not lowered.
- **Someone invited to a sub-project doesn't see the parent.** Invite links and QR codes work at any level: the person joins this sub-project and those nested in it, and only these appear in their list of projects. Someone without access won't even learn that the sub-project exists.
- **A private sub-project** doesn't inherit the parent's members: only those invited to it directly see it — even the parent's owners don't.
- **Directories are shared.** Currencies, rates and the category tree are set in the topmost project and apply to all nested ones. So totals add up honestly: in one base currency and by the same categories.
- **"Require 2FA from members"** on the parent applies to all its sub-projects.
- **Claude and API tokens** get access to the chosen project — with or without its sub-projects, and never more than the user sees.
- **Everything goes into the "Security log":** who gave access to whom, and at which level.

## Development plan

1. **Tree and permissions.** A project gets a parent; nesting goes up to three levels. Access checks take inherited roles and private sub-projects into account, with a test for every rule.
2. **Interface.** A project tree in "Projects & members", creating a sub-project, invites at any level, nesting in the project switcher.
3. **Summary.** The parent's "Overview" shows the total of the nested projects in the base currency, broken down by sub-project. A limit in the parent will be able to count spending in all nested ones — for example, one groceries limit for the whole family.
4. **Transfers between sub-projects** — one transaction with two entries, if you have access to both: for example, from "Home" to the "Kids" cash.
5. **Moving existing projects in.** An existing project can become a sub-project with all its data; matching currencies and categories are merged with the parent's.
6. **Claude and the API.** MCP tools and tokens understand sub-projects: "log it to Kids", "how much did the whole family spend this month".

{% <note kind="info"> %}Nothing changes for those who don't use sub-projects: your projects stay as they are. Each stage ships separately and appears in "What's new".{% </note> %}

## Still to decide

- Whether to show a private sub-project's total in the parent's summary — as one line, without transactions — or not at all.
- Whether a sub-project needs its own categories on top of the shared ones.
- What happens to sub-projects when the parent is deleted: delete them along with it, or move them one level up first.
- Whether three levels of nesting are enough.

If you've ever wanted to show your budget only in part, tell us how it works in your family on the [Reviews](@/feedback.en.md) page. That will decide which stage we start with.
