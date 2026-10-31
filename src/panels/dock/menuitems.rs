use super::taskbar::Window;
use crate::platform::system::desktop::DesktopAction;

#[derive(Debug, PartialEq)]
pub(super) enum MenuItem {
    Window {
        address: String,
        title: String,
    },
    NewWindow,
    Action {
        index: usize,
        name: String,
        icon: String,
    },
    TogglePin {
        is_pinned: bool,
    },
    CloseWindows {
        addresses: Vec<String>,
    },
    OpenTrash,
    EmptyTrash {
        is_enabled: bool,
    },
    Separator,
}

pub(super) fn build_trash_items(item_count: usize) -> Vec<MenuItem> {
    vec![
        MenuItem::OpenTrash,
        MenuItem::EmptyTrash {
            is_enabled: item_count > 0,
        },
    ]
}

pub(super) fn build_items(
    windows: &[Window],
    actions: &[DesktopAction],
    can_launch: bool,
    is_pinned: bool,
) -> Vec<MenuItem> {
    let window_items = windows.iter().map(|window| MenuItem::Window {
        address: window.address.clone(),
        title: if window.title.is_empty() {
            window.class.clone()
        } else {
            window.title.clone()
        },
    });
    let launch_items = can_launch.then_some(MenuItem::NewWindow).into_iter().chain(
        actions
            .iter()
            .enumerate()
            .filter(|(_, action)| !action.name.is_empty() && !action.command.is_empty())
            .map(|(index, action)| MenuItem::Action {
                index,
                name: action.name.clone(),
                icon: action.icon.clone(),
            }),
    );
    let close_items = (!windows.is_empty()).then(|| MenuItem::CloseWindows {
        addresses: windows
            .iter()
            .map(|window| window.address.clone())
            .collect(),
    });
    let groups = [
        window_items.collect::<Vec<_>>(),
        launch_items.collect(),
        vec![MenuItem::TogglePin { is_pinned }],
        close_items.into_iter().collect(),
    ];
    let mut items = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !items.is_empty() {
            items.push(MenuItem::Separator);
        }
        items.extend(group);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_window(address: &str, title: &str) -> Window {
        Window {
            address: address.to_owned(),
            class: "kitty".to_owned(),
            title: title.to_owned(),
            is_active: false,
            is_minimized: false,
            is_urgent: false,
        }
    }

    fn make_action(name: &str, command: &[&str]) -> DesktopAction {
        DesktopAction {
            name: name.to_owned(),
            icon: "tab-new".to_owned(),
            command: command.iter().map(|part| (*part).to_owned()).collect(),
        }
    }

    #[test]
    fn the_menu_lists_windows_then_launchers_then_the_pin_then_closing() {
        let items = build_items(
            &[make_window("0x1", "one"), make_window("0x2", "")],
            &[
                make_action("Private", &["kitty", "--private"]),
                make_action("Broken", &[]),
                make_action("", &["kitty"]),
            ],
            true,
            true,
        );
        assert_eq!(
            items,
            [
                MenuItem::Window {
                    address: "0x1".to_owned(),
                    title: "one".to_owned()
                },
                MenuItem::Window {
                    address: "0x2".to_owned(),
                    title: "kitty".to_owned()
                },
                MenuItem::Separator,
                MenuItem::NewWindow,
                MenuItem::Action {
                    index: 0,
                    name: "Private".to_owned(),
                    icon: "tab-new".to_owned()
                },
                MenuItem::Separator,
                MenuItem::TogglePin { is_pinned: true },
                MenuItem::Separator,
                MenuItem::CloseWindows {
                    addresses: vec!["0x1".to_owned(), "0x2".to_owned()]
                },
            ]
        );
    }

    #[test]
    fn the_trash_menu_offers_emptying_only_while_the_trash_holds_items() {
        assert_eq!(
            [0, 3].map(build_trash_items),
            [
                vec![
                    MenuItem::OpenTrash,
                    MenuItem::EmptyTrash { is_enabled: false }
                ],
                vec![
                    MenuItem::OpenTrash,
                    MenuItem::EmptyTrash { is_enabled: true }
                ],
            ]
        );
    }

    #[test]
    fn an_app_without_windows_or_a_desktop_entry_gets_only_the_pin() {
        assert_eq!(
            build_items(&[], &[], false, false),
            [MenuItem::TogglePin { is_pinned: false }]
        );
    }

    #[test]
    fn an_app_without_windows_can_still_open_a_new_one() {
        assert_eq!(
            build_items(&[], &[], true, false),
            [
                MenuItem::NewWindow,
                MenuItem::Separator,
                MenuItem::TogglePin { is_pinned: false },
            ]
        );
    }
}
