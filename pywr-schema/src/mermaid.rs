#[macro_export]
macro_rules! mermaid {
    ($file:literal)               => { $crate::_mermaid_inner!($file center transparent) };
    ($file:literal left framed)   => { $crate::_mermaid_inner!($file left framed) };
    ($file:literal framed left)   => { $crate::_mermaid_inner!($file left framed) };
    ($file:literal center framed) => { $crate::_mermaid_inner!($file center framed) };
    ($file:literal framed center) => { $crate::_mermaid_inner!($file center framed) };
    ($file:literal right framed)  => { $crate::_mermaid_inner!($file right framed) };
    ($file:literal framed right)  => { $crate::_mermaid_inner!($file right framed) };
    ($file:literal framed)        => { $crate::_mermaid_inner!($file center framed) };
    ($file:literal left)          => { $crate::_mermaid_inner!($file left transparent) };
    ($file:literal right)         => { $crate::_mermaid_inner!($file right transparent) };
    ($file:literal center)        => { $crate::_mermaid_inner!($file center transparent) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! _mermaid_inner {
    ($file:literal $pos:ident $style:ident) => {
        concat!(
            "<pre class=\"mermaid\" style=\"text-align:",
            stringify!($pos),
            ";",
            $crate::_mermaid_background!($style),
            "\">\n",
            include_str!($file),
            "classDef inputNode fill:#377eb8,stroke:#a0a0a0,stroke-width:4px,color:white;",
            "classDef linkNode fill:#fafafa,stroke:#a0a0a0,stroke-width:4px;",
            "classDef outputNode fill:#ffff33,stroke:#a0a0a0,stroke-width:4px;",
            "classDef storageNode fill:#e41a1d,stroke:#a0a0a0,stroke-width:4px,color:white;",
            "classDef aggNode fill:#e8f4f8,stroke:#4a90e2,stroke-width:2px,stroke-dasharray:10,5;",
            "classDef thisNode fill:none,stroke:#000,stroke-width:4px;",
            "classDef slot fill:none,stroke:#a0a0a0,stroke-width:1px;",
            "\n",
            "</pre>",
            "<script type=\"module\">",
            "import mermaid from \"https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs\";",
            "var doc_theme = localStorage.getItem(\"rustdoc-theme\");",
            "if (doc_theme === \"dark\" || doc_theme === \"ayu\") mermaid.initialize({theme: \"dark\"});",
            "</script>"
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! _mermaid_background {
    (framed) => {
        ""
    };
    (transparent) => {
        "background: transparent;"
    };
}
