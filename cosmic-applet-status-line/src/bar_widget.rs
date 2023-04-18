use cosmic::{
    iced::{self, widget, Length, Rectangle},
    iced_core::{
        clipboard::Clipboard,
        event::{self, Event},
        layout::{Layout, Limits, Node},
        mouse,
        renderer::Style,
        touch,
        widget::{
            operation::{Operation, OperationOutputWrapper},
            Tree, Widget,
        },
        Shell,
    },
};

use crate::protocol::ClickEvent;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

/// Wraps a `Row` widget, handling mouse input
pub struct BarWidget<'a, Msg> {
    pub row: widget::Row<'a, Msg, cosmic::Theme, cosmic::Renderer>,
    pub name_instance: Vec<(Option<&'a str>, Option<&'a str>)>,
    pub on_press: fn(ClickEvent) -> Msg,
}

impl<'a, Msg> Widget<Msg, cosmic::Theme, cosmic::Renderer> for BarWidget<'a, Msg> {
    delegate::delegate! {
        to self.row {
            fn children(&self) -> Vec<Tree>;
            fn diff(&mut self, tree: &mut Tree);
            fn layout(&self, tree: &mut Tree, renderer: &cosmic::Renderer, limits: &Limits) -> Node;
            fn operate(
                &self,
                tree: &mut Tree,
                layout: Layout<'_>,
                renderer: &cosmic::Renderer,
                operation: &mut dyn Operation<OperationOutputWrapper<Msg>>,
            );
            fn draw(
                &self,
                state: &Tree,
                renderer: &mut cosmic::Renderer,
                theme: &cosmic::Theme,
                style: &Style,
                layout: Layout,
                cursor: iced::mouse::Cursor,
                viewport: &Rectangle,
            );
        }
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: iced::mouse::Cursor,
        renderer: &cosmic::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Msg>,
        viewport: &Rectangle,
    ) -> event::Status {
        if self.update(&event, layout, cursor, shell) == event::Status::Captured {
            return event::Status::Captured;
        }
        self.row.on_event(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
        )
    }

    fn size(&self) -> iced::Size<Length> {
        Widget::size(&self.row)
    }
}

impl<'a, Msg> From<BarWidget<'a, Msg>> for cosmic::Element<'a, Msg>
where
    Msg: 'a,
{
    fn from(widget: BarWidget<'a, Msg>) -> cosmic::Element<'a, Msg> {
        cosmic::Element::new(widget)
    }
}

impl<'a, Msg> BarWidget<'a, Msg> {
    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: iced::mouse::Cursor,
        shell: &mut Shell<'_, Msg>,
    ) -> event::Status {
        let Some(cursor_position) = cursor.position() else {
            return event::Status::Ignored;
        };

        let (button, event_code) = match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => (1, BTN_LEFT),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => (2, BTN_MIDDLE),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => (3, BTN_RIGHT),
            Event::Touch(touch::Event::FingerPressed { .. }) => (1, BTN_LEFT),
            _ => {
                return event::Status::Ignored;
            }
        };

        let Some((n, bounds)) = layout
            .children()
            .map(|x| x.bounds())
            .enumerate()
            .find(|(_, bounds)| bounds.contains(cursor_position))
        else {
            return event::Status::Ignored;
        };

        let (name, instance) = self.name_instance.get(n).cloned().unwrap_or((None, None));

        // TODO coordinate space? int conversion?
        let x = cursor_position.x as u32;
        let y = cursor_position.y as u32;
        let relative_x = (cursor_position.x - bounds.x) as u32;
        let relative_y = (cursor_position.y - bounds.y) as u32;
        let width = bounds.width as u32;
        let height = bounds.height as u32;

        shell.publish((self.on_press)(ClickEvent {
            name: name.map(str::to_owned),
            instance: instance.map(str::to_owned),
            x,
            y,
            button,
            event: event_code,
            relative_x,
            relative_y,
            width,
            height,
        }));

        event::Status::Captured
    }
}
