use barepdf_core::PageIndex;

pub(crate) fn plain_text(pages: &[(PageIndex, String)]) -> String {
    let mut output = pages
        .iter()
        .map(|(_, text)| text.trim())
        .collect::<Vec<_>>()
        .join("\n\n");
    output.push('\n');
    output
}

pub(crate) fn markdown(pages: &[(PageIndex, String)]) -> String {
    let mut output = String::new();
    for (position, (page_index, text)) in pages.iter().enumerate() {
        if position != 0 {
            output.push_str("\n\n---\n\n");
        }
        output.push_str("<!-- Page ");
        output.push_str(&page_index.to_string());
        output.push_str(" -->\n\n");
        output.push_str(text.trim());
    }
    output.push('\n');
    output
}
