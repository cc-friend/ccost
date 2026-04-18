use super::table::{format_cost, format_tokens};
use crate::types::{GroupedData, PriceMode};

pub struct HtmlOptions {
    pub dimension_label: String,
    pub price_mode: PriceMode,
    pub compact: bool,
    pub title: Option<String>,
}

/// HTML-escape user content.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Format a cell value: tokens with optional cost in a span.
fn format_cell_html(tokens: u64, cost: f64, price_mode: PriceMode) -> String {
    let token_str = html_escape(&format_tokens(tokens));
    if price_mode == PriceMode::Off {
        token_str
    } else {
        let cost_str = html_escape(&format_cost(cost, price_mode));
        format!("{} <span class=\"cost\">({})</span>", token_str, cost_str)
    }
}

struct RowData {
    label: String,
    cells: Vec<String>,
}

fn build_row_html(
    entry: &GroupedData,
    price_mode: PriceMode,
    compact: bool,
    label_prefix: &str,
) -> RowData {
    let in_total = entry.input_tokens + entry.cache_creation_tokens + entry.cache_read_tokens;
    let in_total_cost = entry.input_cost + entry.cache_creation_cost + entry.cache_read_cost;
    let total = in_total + entry.output_tokens;
    let total_cost = entry.total_cost;

    let label = format!("{}{}", label_prefix, html_escape(&entry.label));

    let cells = if compact {
        vec![
            format_cell_html(in_total, in_total_cost, price_mode),
            format_cell_html(entry.output_tokens, entry.output_cost, price_mode),
            format_cell_html(total, total_cost, price_mode),
        ]
    } else {
        vec![
            format_cell_html(entry.input_tokens, entry.input_cost, price_mode),
            format_cell_html(
                entry.cache_creation_tokens,
                entry.cache_creation_cost,
                price_mode,
            ),
            format_cell_html(entry.cache_read_tokens, entry.cache_read_cost, price_mode),
            format_cell_html(in_total, in_total_cost, price_mode),
            format_cell_html(entry.output_tokens, entry.output_cost, price_mode),
            format_cell_html(total, total_cost, price_mode),
        ]
    };

    RowData { label, cells }
}

pub fn format_html(data: &[GroupedData], totals: &GroupedData, options: &HtmlOptions) -> String {
    let is_default_title = options.title.is_none();
    let title = options.title.as_deref().unwrap_or("Report by ccost");

    let headers: Vec<&str> = if options.compact {
        vec![&options.dimension_label, "Input Total", "Output", "Total"]
    } else {
        vec![
            &options.dimension_label,
            "Input",
            "Cache Creation",
            "Cache Read",
            "Input Total",
            "Output",
            "Total",
        ]
    };

    let num_cols = headers.len();

    let mut html = String::new();

    // DOCTYPE and head
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"UTF-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n<title>");
    html.push_str(&html_escape(title));
    html.push_str("</title>\n<script>");
    html.push_str(THEME_BOOT_JS);
    html.push_str("</script>\n<style>\n");
    html.push_str(CSS);
    html.push_str("\n</style>\n</head>\n<body>\n");

    // Theme toggle
    html.push_str("<button id=\"theme-toggle\" class=\"theme-toggle\" type=\"button\" aria-label=\"Toggle theme\">Light</button>\n");

    // h1
    if is_default_title {
        html.push_str("<h1>Report by <a href=\"https://github.com/cc-friend/ccost\" target=\"_blank\" rel=\"noopener noreferrer\">ccost</a></h1>\n");
    } else {
        html.push_str("<h1>");
        html.push_str(&html_escape(title));
        html.push_str("</h1>\n");
    }

    // Table
    html.push_str("<table>\n<thead>\n<tr>\n");
    for (i, header) in headers.iter().enumerate() {
        html.push_str(&format!(
            "<th class=\"sortable\" data-col=\"{}\">{}<span class=\"sort-arrow\"><svg width=\"12\" height=\"14\" viewBox=\"0 0 12 14\"><path d=\"M6 0L12 6H0z\" class=\"arrow-up\"/><path d=\"M6 14L0 8h12z\" class=\"arrow-down\"/></svg></span></th>\n",
            i,
            html_escape(header)
        ));
    }
    html.push_str("</tr>\n</thead>\n<tbody>\n");

    // Data rows
    for entry in data {
        let row = build_row_html(entry, options.price_mode, options.compact, "");
        html.push_str("<tr class=\"parent\">\n");
        html.push_str(&format!("<td>{}</td>\n", row.label));
        for cell in &row.cells {
            html.push_str(&format!("<td>{}</td>\n", cell));
        }
        html.push_str("</tr>\n");

        if let Some(ref children) = entry.children {
            for child in children {
                let child_row = build_row_html(
                    child,
                    options.price_mode,
                    options.compact,
                    "\u{2514}\u{2500} ",
                );
                html.push_str("<tr class=\"child\">\n");
                html.push_str(&format!("<td>{}</td>\n", child_row.label));
                for cell in &child_row.cells {
                    html.push_str(&format!("<td>{}</td>\n", cell));
                }
                html.push_str("</tr>\n");
            }
        }
    }

    html.push_str("</tbody>\n<tfoot>\n");

    // Totals row
    let totals_row = build_row_html(totals, options.price_mode, options.compact, "");
    html.push_str("<tr class=\"totals totals-main\">\n");
    html.push_str("<td>TOTAL</td>\n");
    for cell in &totals_row.cells {
        html.push_str(&format!("<td>{}</td>\n", cell));
    }
    html.push_str("</tr>\n");

    // Totals children
    if let Some(ref children) = totals.children {
        for child in children {
            let child_row = build_row_html(
                child,
                options.price_mode,
                options.compact,
                "\u{2514}\u{2500} ",
            );
            html.push_str("<tr class=\"totals totals-child\">\n");
            html.push_str(&format!("<td>{}</td>\n", child_row.label));
            for cell in &child_row.cells {
                html.push_str(&format!("<td>{}</td>\n", cell));
            }
            html.push_str("</tr>\n");
        }
    }

    html.push_str("</tfoot>\n</table>\n");

    // JavaScript
    html.push_str("<script>\n");
    html.push_str(&build_js(num_cols));
    html.push_str("\n</script>\n");

    html.push_str("</body>\n</html>\n");

    html
}

const THEME_BOOT_JS: &str = r#"(function(){try{var s=localStorage.getItem('ccost-theme');if(s==='light'||s==='dark'){document.documentElement.setAttribute('data-theme',s);}else if(window.matchMedia&&window.matchMedia('(prefers-color-scheme: light)').matches){document.documentElement.setAttribute('data-theme','light');}}catch(e){}})();"#;

const CSS: &str = r#":root {
  color-scheme: dark;
  --bg: #0d1117;
  --text: #c9d1d9;
  --accent: #58a6ff;
  --border: #30363d;
  --thead-bg: #161b22;
  --thead-hover: #1f252c;
  --parent-bg: #1a1f26;
  --parent-hover: #22272e;
  --child-bg: #161b22;
  --totals-bg: #1f252c;
  --cost: #3fb950;
  --arrow: #6e7681;
}
:root[data-theme="light"] {
  color-scheme: light;
  --bg: #ffffff;
  --text: #24292f;
  --accent: #0969da;
  --border: #d0d7de;
  --thead-bg: #f6f8fa;
  --thead-hover: #eaeef2;
  --parent-bg: #ffffff;
  --parent-hover: #f6f8fa;
  --child-bg: #f6f8fa;
  --totals-bg: #eaeef2;
  --cost: #1a7f37;
  --arrow: #8c959f;
}
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}
body {
  background: var(--bg);
  color: var(--text);
  font-family: 'Segoe UI', Tahoma, Geneva, Verdana, sans-serif;
  padding: 2rem;
}
h1 {
  color: var(--text);
  margin-bottom: 1.5rem;
  font-size: 1.5rem;
}
a {
  color: #CC5B4F;
  text-decoration: underline;
  transition: opacity 0.2s;
}
a:hover {
  opacity: 0.65;
}
h1 a {
  text-decoration: none;
}
.theme-toggle {
  position: fixed;
  top: 1rem;
  right: 1rem;
  background: var(--thead-bg);
  color: var(--accent);
  border: 1px solid var(--border);
  padding: 0.35rem 0.7rem;
  border-radius: 4px;
  cursor: pointer;
  font-size: 0.8rem;
  font-family: inherit;
  z-index: 10;
}
.theme-toggle:hover {
  background: var(--thead-hover);
}
table {
  border-collapse: collapse;
  width: 100%;
  font-size: 0.9rem;
}
th, td {
  padding: 0.6rem 1rem;
  border: 1px solid var(--border);
  text-align: right;
}
th:first-child, td:first-child {
  text-align: left;
}
thead th {
  background: var(--thead-bg);
  color: var(--accent);
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
}
thead th:hover {
  background: var(--thead-hover);
}
tbody, tfoot {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', monospace;
}
tbody tr.parent {
  background: var(--parent-bg);
}
tbody tr.parent:hover {
  background: var(--parent-hover);
}
tbody tr.child {
  background: var(--child-bg);
  font-size: 0.85rem;
}
tbody tr.child td:first-child {
  padding-left: 2rem;
}
tfoot tr.totals {
  background: var(--totals-bg);
  font-weight: bold;
}
tfoot tr.totals-main {
  color: var(--accent);
}
tfoot tr.totals-child {
  font-weight: normal;
  font-size: 0.85rem;
}
tfoot tr.totals-child td:first-child {
  padding-left: 2rem;
}
.cost {
  color: var(--cost);
  font-size: 0.85em;
}
.sort-arrow {
  display: inline-block;
  margin-left: 4px;
  vertical-align: middle;
}
.sort-arrow svg {
  display: block;
}
.arrow-up, .arrow-down {
  fill: var(--arrow);
  transition: fill 0.2s;
}
th.sort-asc .arrow-up {
  fill: var(--accent);
}
th.sort-desc .arrow-down {
  fill: var(--accent);
}
@media print {
  :root, :root[data-theme="light"], :root[data-theme="dark"] {
    --bg: #ffffff;
    --text: #000000;
    --accent: #000000;
    --border: #999999;
    --thead-bg: #eeeeee;
    --thead-hover: #eeeeee;
    --parent-bg: #ffffff;
    --parent-hover: #ffffff;
    --child-bg: #f6f6f6;
    --totals-bg: #dddddd;
    --cost: #555555;
    --arrow: transparent;
  }
  body {
    padding: 0;
    font-size: 11pt;
  }
  h1 {
    font-size: 14pt;
    margin-bottom: 0.5rem;
  }
  table {
    font-size: 9.5pt;
  }
  thead th {
    cursor: default;
  }
  thead {
    display: table-header-group;
  }
  tr {
    page-break-inside: avoid;
  }
  a {
    color: inherit;
    text-decoration: none;
  }
  .theme-toggle, .sort-arrow {
    display: none;
  }
}"#;

fn build_js(_num_cols: usize) -> String {
    r#"(function() {
  const root = document.documentElement;
  const btn = document.getElementById('theme-toggle');
  if (btn) {
    function refreshLabel() {
      const cur = root.getAttribute('data-theme') === 'light' ? 'light' : 'dark';
      btn.textContent = cur === 'light' ? 'Dark' : 'Light';
    }
    refreshLabel();
    btn.addEventListener('click', () => {
      const next = root.getAttribute('data-theme') === 'light' ? 'dark' : 'light';
      root.setAttribute('data-theme', next);
      try { localStorage.setItem('ccost-theme', next); } catch(e) {}
      refreshLabel();
    });
  }

  const table = document.querySelector('table');
  const thead = table.querySelector('thead');
  const tbody = table.querySelector('tbody');
  const ths = thead.querySelectorAll('th');
  let sortState = {};

  function getGroups() {
    const rows = Array.from(tbody.querySelectorAll('tr'));
    const groups = [];
    let current = null;
    for (const row of rows) {
      const firstCell = row.querySelector('td');
      const text = firstCell ? firstCell.textContent : '';
      if (row.classList.contains('child') || text.startsWith('\u{2514}')) {
        if (current) current.children.push(row);
      } else {
        current = { parent: row, children: [] };
        groups.push(current);
      }
    }
    return groups;
  }

  function sfx(n, s) {
    if (!s) return n;
    s = s.toUpperCase();
    if (s === 'K') return n * 1e3;
    if (s === 'M') return n * 1e6;
    if (s === 'G' || s === 'B') return n * 1e9;
    return n;
  }

  function parseValue(text) {
    const t = text.replace(/\(.*?\)/g, '').trim();
    if (t === '\u2014' || t === '' || t === '-') return NaN;
    // Dollar: $1.23 or $1.2K
    let m = t.match(/^\$([\d,.]+)\s*([KMGB])?$/i);
    if (m) return sfx(parseFloat(m[1].replace(/,/g, '')), m[2]);
    // Duration: 1d 2h 30m 15s (any combo)
    m = t.match(/^(?:(\d+)d\s*)?(?:(\d+)h\s*)?(?:(\d+)m\s*)?(?:(\d+)s)?$/);
    if (m && (m[1]||m[2]||m[3]||m[4]))
      return ((+m[1]||0)*86400)+((+m[2]||0)*3600)+((+m[3]||0)*60)+(+m[4]||0);
    // Pct range: 10%–25% or 10% — sort by max
    m = t.match(/([\d.]+)%/g);
    if (m) return parseFloat(m[m.length - 1]);
    // Lines: +123 -45
    m = t.match(/^\+([\d,]+)\s+-([\d,]+)$/);
    if (m) return parseInt(m[1].replace(/,/g,'')) + parseInt(m[2].replace(/,/g,''));
    // Plain number with optional suffix: 1,200 or 1.2K
    m = t.match(/^([\d,.]+)\s*([KMGB])?$/i);
    if (m) return sfx(parseFloat(m[1].replace(/,/g, '')), m[2]);
    return NaN;
  }

  function getCellValue(row, col) {
    const cells = row.querySelectorAll('td');
    if (col >= cells.length) return '';
    return cells[col].textContent || '';
  }

  const originalGroups = getGroups().map((g, i) => ({ ...g, index: i }));

  ths.forEach((th, colIdx) => {
    th.addEventListener('click', () => {
      const prev = sortState[colIdx] || 'none';
      let next;
      if (prev === 'none') next = 'asc';
      else if (prev === 'asc') next = 'desc';
      else next = 'none';

      // Clear all sort classes
      ths.forEach(t => {
        t.classList.remove('sort-asc', 'sort-desc');
      });
      sortState = {};

      if (next !== 'none') {
        sortState[colIdx] = next;
        th.classList.add('sort-' + next);
      }

      let groups = originalGroups.map(g => ({ ...g }));

      if (next !== 'none') {
        groups.sort((a, b) => {
          const aText = getCellValue(a.parent, colIdx);
          const bText = getCellValue(b.parent, colIdx);
          const aNum = parseValue(aText);
          const bNum = parseValue(bText);
          let cmp;
          if (!isNaN(aNum) && !isNaN(bNum)) {
            cmp = aNum - bNum;
          } else {
            cmp = aText.localeCompare(bText);
          }
          return next === 'desc' ? -cmp : cmp;
        });
      }

      // Rebuild tbody
      while (tbody.firstChild) tbody.removeChild(tbody.firstChild);
      for (const g of groups) {
        tbody.appendChild(g.parent);
        for (const child of g.children) {
          tbody.appendChild(child);
        }
      }
    });
  });
})();"#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_escape() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
        assert_eq!(html_escape("a&b"), "a&amp;b");
        assert_eq!(html_escape("say \"hi\""), "say &quot;hi&quot;");
    }

    #[test]
    fn test_format_html_basic() {
        let data = vec![GroupedData {
            label: "2025-01".to_string(),
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 200,
            cache_read_tokens: 300,
            input_cost: 0.10,
            cache_creation_cost: 0.02,
            cache_read_cost: 0.03,
            output_cost: 0.05,
            total_cost: 0.20,
            children: None,
        }];
        let totals = data[0].clone();

        let options = HtmlOptions {
            dimension_label: "Month".to_string(),
            price_mode: PriceMode::Off,
            compact: false,
            title: None,
        };

        let result = format_html(&data, &totals, &options);
        assert!(result.contains("<!DOCTYPE html>"));
        assert!(result.contains("Report by <a"));
        assert!(result.contains("href=\"https://github.com/cc-friend/ccost\""));
        assert!(result.contains("target=\"_blank\""));
        assert!(result.contains("rel=\"noopener noreferrer\""));
        assert!(result.contains(">ccost</a>"));
        assert!(result.contains("<thead>"));
        assert!(result.contains("<tbody>"));
        assert!(result.contains("<tfoot>"));
        assert!(result.contains("class=\"parent\""));
        assert!(result.contains("class=\"totals totals-main\""));
        assert!(result.contains("sortable"));
    }

    #[test]
    fn test_format_html_custom_title() {
        let data = vec![];
        let totals = GroupedData {
            label: "TOTAL".to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            input_cost: 0.0,
            cache_creation_cost: 0.0,
            cache_read_cost: 0.0,
            output_cost: 0.0,
            total_cost: 0.0,
            children: None,
        };

        let options = HtmlOptions {
            dimension_label: "Month".to_string(),
            price_mode: PriceMode::Off,
            compact: true,
            title: Some("My Custom Report".to_string()),
        };

        let result = format_html(&data, &totals, &options);
        assert!(result.contains("My Custom Report"));
        // Custom title should NOT have the link
        assert!(!result.contains("https://github.com/cc-friend/ccost"));
    }

    #[test]
    fn test_html_css_tbody_font() {
        let html = CSS;
        assert!(
            html.contains("tbody") && html.contains("tfoot"),
            "CSS should have tbody/tfoot rule"
        );
        assert!(
            html.contains("-apple-system") && html.contains("monospace"),
            "tbody/tfoot should use -apple-system, monospace font stack"
        );
    }

    #[test]
    fn test_html_css_cost_color() {
        let html = CSS;
        assert!(
            html.contains("--cost: #3fb950;"),
            "dark theme --cost should be #3fb950"
        );
        assert!(
            html.contains(".cost {") && html.contains("color: var(--cost);"),
            "cost spans should reference the --cost variable"
        );
    }

    #[test]
    fn test_html_theme_toggle_present() {
        let data = vec![];
        let totals = GroupedData {
            label: "TOTAL".to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            input_cost: 0.0,
            cache_creation_cost: 0.0,
            cache_read_cost: 0.0,
            output_cost: 0.0,
            total_cost: 0.0,
            children: None,
        };
        let options = HtmlOptions {
            dimension_label: "Day".to_string(),
            price_mode: PriceMode::Off,
            compact: true,
            title: None,
        };
        let result = format_html(&data, &totals, &options);
        assert!(
            result.contains("id=\"theme-toggle\""),
            "should render theme toggle button"
        );
        assert!(
            result.contains("ccost-theme"),
            "should persist theme choice via localStorage key 'ccost-theme'"
        );
        assert!(
            result.contains("prefers-color-scheme: light"),
            "should honor OS-level light-mode preference"
        );
    }

    #[test]
    fn test_html_css_print_rules() {
        let html = CSS;
        assert!(html.contains("@media print"), "should include @media print");
        assert!(
            html.contains(".theme-toggle, .sort-arrow {") && html.contains("display: none;"),
            "toggle and sort arrows should be hidden when printing"
        );
        assert!(
            html.contains("page-break-inside: avoid;"),
            "rows should avoid splitting across pages"
        );
    }

    #[test]
    fn test_html_css_light_theme_vars() {
        let html = CSS;
        assert!(
            html.contains(":root[data-theme=\"light\"]"),
            "light theme should be selectable via data-theme='light'"
        );
    }

    #[test]
    fn test_html_sort_arrow_spacing() {
        let data = vec![];
        let totals = GroupedData {
            label: "TOTAL".to_string(),
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            input_cost: 0.0,
            cache_creation_cost: 0.0,
            cache_read_cost: 0.0,
            output_cost: 0.0,
            total_cost: 0.0,
            children: None,
        };
        let options = HtmlOptions {
            dimension_label: "Day".to_string(),
            price_mode: PriceMode::Off,
            compact: false,
            title: None,
        };
        let result = format_html(&data, &totals, &options);
        // SVG height should be 14 (not 12) to have gap between arrows
        assert!(
            result.contains("height=\"14\""),
            "sort arrow SVG should have height 14 for spacing"
        );
        // Down arrow should start at y=8 (gap from y=6 to y=8)
        assert!(
            result.contains("M6 14L0 8h12z"),
            "down arrow path should be offset to create gap"
        );
    }

    #[test]
    fn test_html_cost_span_color() {
        let data = vec![GroupedData {
            label: "2025-01".to_string(),
            input_tokens: 1000,
            output_tokens: 500,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            input_cost: 5.0,
            cache_creation_cost: 0.0,
            cache_read_cost: 0.0,
            output_cost: 5.0,
            total_cost: 10.0,
            children: None,
        }];
        let totals = data[0].clone();
        let options = HtmlOptions {
            dimension_label: "Month".to_string(),
            price_mode: PriceMode::Integer,
            compact: true,
            title: None,
        };
        let result = format_html(&data, &totals, &options);
        assert!(
            result.contains("<span class=\"cost\">"),
            "should contain cost spans when price_mode is not Off"
        );
    }
}
