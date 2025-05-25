use maud::{Markup, html};

#[bon::builder]
pub(crate) fn labeled_input(
    name: &str,
    label: &str,
    id: Option<&str>,
    r#type: &str,
    placeholder: Option<&str>,
    required: Option<bool>,
) -> Markup {
    let id = id.unwrap_or(name);
    let required = required.unwrap_or_default();

    html! {
        label for=(name) { (label) }
        input type=(r#type) id=(id) name=(name) placeholder=[placeholder] required[required];
    }
}
