+++
title = "Category search ignores accents"
date = 2026-10-19

[taxonomies]
topics = ["claude"]

[extra]
change = "improved"
+++
Category search in quick entry ignores case and accents: "cafe" finds "Café", and "елка" finds "Ёлка". Claude finds a category by its exact name in any language — "รถ" is no longer confused with "ล้างรถ", for example. Codes of new categories, accounts and currencies take only Latin letters, digits and _, so they always work in links, and records with a "/" in their code open again.
