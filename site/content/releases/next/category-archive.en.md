+++
title = "A category with subcategories saves again"
date = 2026-10-20

[taxonomies]
topics = ["start"]

[extra]
change = "fixed"
+++
Clicking "Archive" on a category with subcategories was refused — as intended — but afterwards "Save" tried to archive the category again, and edits to its name were lost. Now you stay on the category page, the reason appears in a pop-up message in your interface language, and "Save" saves.
