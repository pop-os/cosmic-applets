//! A container for capturing mouse events.

use cosmic::iced::Vector;

use cosmic::iced::core::{
    Clipboard, Element, Layout, Length, Rectangle, Shell, Size, Widget,
    event::Event,
    layout, mouse, overlay, renderer,
    widget::{Operation, Tree, tree},
};

/// Emit messages on mouse events.
#[allow(missing_debug_implementations)]
pub struct MouseArea<'a, Message, Theme = crate::Theme, Renderer = crate::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    on_mouse_wheel: Option<Box<dyn Fn(mouse::ScrollDelta) -> Message + 'a>>,
}

impl<'a, Message, Theme, Renderer> MouseArea<'a, Message, Theme, Renderer> {
    #[must_use]
    /// The message to emit when the mouse wheel is released.
    pub fn on_mouse_wheel(mut self, message: impl Fn(mouse::ScrollDelta) -> Message + 'a) -> Self {
        self.on_mouse_wheel = Some(Box::new(message));
        self
    }
}

/// Local state of the [`MouseArea`].
#[derive(Default)]
struct State {}

impl<'a, Message, Theme, Renderer> MouseArea<'a, Message, Theme, Renderer> {
    /// Creates a [`MouseArea`] with the given content.
    pub fn new(content: impl Into<Element<'a, Message, Theme, Renderer>>) -> Self {
        MouseArea {
            content: content.into(),
            on_mouse_wheel: None,
        }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for MouseArea<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
    Message: Clone,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_mut(&mut self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation<()>,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        if shell.is_event_captured() {
            return;
        }

        update(
            self,
            event,
            layout,
            cursor,
            shell,
            tree.state.downcast_mut::<State>(),
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        renderer_style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            renderer_style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout,
            renderer,
            viewport,
            translation,
        )
    }

    fn drag_destinations(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        dnd_rectangles: &mut cosmic::iced::core::clipboard::DndDestinationRectangles,
    ) {
        if let Some(state) = state.children.first() {
            self.content
                .as_widget()
                .drag_destinations(state, layout, renderer, dnd_rectangles);
        }
    }
}

impl<'a, Message, Theme, Renderer> From<MouseArea<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a + Clone,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(
        area: MouseArea<'a, Message, Theme, Renderer>,
    ) -> Element<'a, Message, Theme, Renderer> {
        Element::new(area)
    }
}

/// Processes the given [`Event`] and updates the [`State`] of an [`MouseArea`]
/// accordingly.
fn update<Message: Clone, Theme, Renderer>(
    widget: &mut MouseArea<'_, Message, Theme, Renderer>,
    event: &Event,
    layout: Layout<'_>,
    cursor: mouse::Cursor,
    shell: &mut Shell<'_, Message>,
    _state: &mut State,
) {
    if !cursor.is_over(layout.bounds()) {
        return;
    }

    if let Some(message) = widget.on_mouse_wheel.as_ref()
        && let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event
    {
        shell.publish((message)(*delta));
        shell.capture_event();
    }
}
