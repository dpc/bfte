use maud::{Markup, html};

#[bon::builder]
pub(crate) fn labeled_input(
    name: &str,
    label: &str,
    id: Option<&str>,
    r#type: &str,
    placeholder: Option<&str>,
    bind: Option<&str>,
    required: Option<bool>,
) -> Markup {
    let id = id.unwrap_or(name);
    let required = required.unwrap_or_default();

    html! {
        label for=(name) { (label) }
        input
            type=(r#type) id=(id) name=(name) placeholder=[placeholder] required[required]
            data-bind=[bind];
    }
}

#[bon::builder]
pub(crate) fn labeled_textarea(
    name: &str,
    label: &str,
    id: Option<&str>,
    placeholder: Option<&str>,
    bind: Option<&str>,
    required: Option<bool>,
) -> Markup {
    let id = id.unwrap_or(name);
    let required = required.unwrap_or_default();

    html! {
        label for=(name) { (label) }
        textarea
            id=(id) name=(name) placeholder=[placeholder] required[required]
            data-bind=[bind] {}
    }
}
