# Vision

Effigy gives a monorepo one predictable command surface. A manifest owns task selection and execution; built-ins cover common operational jobs without hiding repository-specific tasks. The same selector and context should resolve the same way, and ambiguity should fail with usable evidence.

Text, JSON, and managed task interfaces should express the same decision. Versioned JSON contracts support automation. Fast discovery and bounded diagnosis matter because Effigy is used by agents as well as people.

Keep the runner reusable. Provider-specific deployment behavior and consumer application policy belong outside the core. Grow features through explicit ownership and compatibility decisions, with release gates proving the shipped surface.

The [architecture index](architecture.md) and [contract index](contracts/README.md) own the corresponding technical details.
