use crate::markdown;

pub fn standalone_html(title: &str, source: &str, dark: bool) -> String {
    let body = markdown::to_html(source);
    let escaped_title = escape(title);
    let (bg, fg, muted, border, card, accent) = if dark {
        ("#08080a", "#e8e8ea", "#a9a9ae", "#26262b", "#111114", "#8b7cf6")
    } else {
        ("#ffffff", "#1f1f24", "#5f5f68", "#e4e4e8", "#fafafb", "#6d5ce6")
    };
    let mermaid_theme = if dark { "dark" } else { "default" };
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{escaped_title}</title>
<style>
:root {{ color-scheme: {scheme}; }}
body {{ margin: 0; background: {bg}; color: {fg}; font: 16px/1.65 -apple-system, "Segoe UI", Inter, Roboto, sans-serif; }}
main {{ max-width: 760px; margin: 0 auto; padding: 48px 24px 120px; }}
h1, h2, h3, h4, h5, h6 {{ line-height: 1.25; margin: 1.6em 0 .5em; }}
h1 {{ font-size: 2em; }} h2 {{ font-size: 1.5em; border-bottom: 1px solid {border}; padding-bottom: .3em; }}
a {{ color: {accent}; }}
code {{ font-family: "JetBrains Mono", "Cascadia Code", Consolas, monospace; font-size: .9em; background: {card}; padding: .1em .35em; border-radius: 5px; }}
pre {{ background: {card}; border: 1px solid {border}; border-radius: 10px; padding: 14px 16px; overflow: auto; }}
pre code {{ background: none; padding: 0; }}
blockquote {{ margin: 0; padding: .2em 1em; border-left: 3px solid {accent}; color: {muted}; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ border: 1px solid {border}; padding: 8px 12px; text-align: left; }}
th {{ background: {card}; }}
img {{ max-width: 100%; border-radius: 10px; }}
hr {{ border: 0; border-top: 1px solid {border}; margin: 2em 0; }}
input[type=checkbox] {{ accent-color: {accent}; }}
.mermaid {{ background: {card}; border: 1px solid {border}; border-radius: 10px; padding: 16px; }}
</style>
</head>
<body>
<main>
{body}
</main>
<script type="module">
for (const block of document.querySelectorAll("pre > code.language-mermaid")) {{
  const pre = block.parentElement;
  const div = document.createElement("div");
  div.className = "mermaid";
  div.textContent = block.textContent;
  pre.replaceWith(div);
}}
if (document.querySelector(".mermaid")) {{
  const mermaid = (await import("https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs")).default;
  mermaid.initialize({{ startOnLoad: false, theme: "{mermaid_theme}", securityLevel: "strict" }});
  await mermaid.run();
}}
</script>
</body>
</html>
"#,
        scheme = if dark { "dark" } else { "light" },
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
