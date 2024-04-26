use cosmic::{
    desktop::IconSource,
    iced::Limits,
    iced_core::{layout, overlay, widget::Tree, Border, Layout, Length, Size, Vector},
    theme::{Button, Container},
    widget::{button, container, image::Handle, Image, Widget},
    Element,
};

use crate::wayland_subscription::WaylandImage;

pub struct WindowImage<'a, Msg> {
    image_button: Element<'a, Msg>,
    icon: Element<'a, Msg>,
}

impl<'a, Msg> WindowImage<'a, Msg>
where
    Msg: 'static + Clone,
{
    pub fn new(
        img: Option<WaylandImage>,
        icon: &IconSource,
        size: f32,
        on_press: Msg,
        padding: u16,
    ) -> Self {
        let border = 1.0;
        Self {
            image_button: button(
                container(
                    container(if let Some(img) = img {
                        let max_dim = img.img.width().max(img.img.height()).max(1);
                        let ratio = max_dim as f32 / (size - border * 2.0).max(1.0);
                        let adjusted_width = img.img.width() as f32 / ratio;
                        let adjusted_height = img.img.height() as f32 / ratio;

                        Element::from(
                            Image::new(Handle::from_pixels(
                                img.img.width(),
                                img.img.height(),
                                img.clone(),
                            ))
                            .width(Length::Fixed(adjusted_width))
                            .height(Length::Fixed(adjusted_height))
                            .content_fit(cosmic::iced_core::ContentFit::Contain),
                        )
                    } else {
                        Element::from(
                            icon.as_cosmic_icon()
                                .width(Length::Fixed((size - border * 2.0).max(0.)))
                                .height(Length::Fixed((size - border * 2.0).max(0.))),
                        )
                    })
                    .style(Container::Custom(Box::new(move |theme| {
                        container::Appearance {
                            border: Border {
                                color: theme.cosmic().bg_divider().into(),
                                width: border,
                                radius: 0.0.into(),
                            },
                            ..Default::default()
                        }
                    })))
                    .padding(border as u16)
                    .height(Length::Shrink)
                    .width(Length::Shrink),
                )
                .align_x(cosmic::iced_core::alignment::Horizontal::Center)
                .align_y(cosmic::iced_core::alignment::Vertical::Center)
                .height(Length::Fixed(size + padding as f32 * 2.0))
                .width(Length::Fixed(size + padding as f32 * 2.0))
                .padding(padding),
            )
            .on_press(on_press)
            .width(Length::Shrink)
            .height(Length::Shrink)
            .style(Button::AppletIcon)
            .padding(0)
            .into(),
            icon: icon
                .as_cosmic_icon()
                .width(Length::Fixed(size / 3.0))
                .height(Length::Fixed(size / 3.0))
                .into(),
        }
    }
}

impl<'a, Msg> Widget<Msg, cosmic::Theme, cosmic::Renderer> for WindowImage<'a, Msg> {
    fn children(&self) -> Vec<cosmic::iced_core::widget::Tree> {
        vec![Tree::new(&self.image_button), Tree::new(&self.icon)]
    }

    fn diff(&mut self, tree: &mut cosmic::iced_core::widget::Tree) {
        tree.diff_children(&mut [&mut self.image_button, &mut self.icon])
    }

    fn overlay<'b>(
        &'b mut self,
        state: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &cosmic::Renderer,
    ) -> Option<cosmic::iced_core::overlay::Element<'b, Msg, cosmic::Theme, cosmic::Renderer>> {
        let children = [&mut self.image_button, &mut self.icon]
            .into_iter()
            .zip(&mut state.children)
            .zip(layout.children())
            .filter_map(|((child, state), layout)| {
                child.as_widget_mut().overlay(state, layout, renderer)
            })
            .collect::<Vec<_>>();

        (!children.is_empty()).then(|| overlay::Group::with_children(children).overlay())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &self,
        tree: &mut cosmic::iced_core::widget::Tree,
        renderer: &cosmic::Renderer,
        limits: &cosmic::iced_core::layout::Limits,
    ) -> cosmic::iced_core::layout::Node {
        let children = &mut tree.children;
        let button = &mut children[0];
        let button_node = self
            .image_button
            .as_widget()
            .layout(button, renderer, limits);
        let img_node = &button_node.children()[0].children()[0];

        let button_bounds = img_node.size();
        let icon_width = button_bounds.width.max(button_bounds.height) / 3.0;
        let icon_height = button_bounds.height.max(button_bounds.width) / 3.0;
        let icon = &mut children[1];
        let icon_node = self
            .icon
            .as_widget()
            .layout(
                icon,
                renderer,
                &Limits::NONE.width(icon_width).height(icon_height),
            )
            .translate(Vector::new(
                img_node.bounds().x + 2. * button_bounds.width / 3.0,
                img_node.bounds().y + 2. * button_bounds.height / 3.0,
            ));

        layout::Node::with_children(
            limits.resolve(Length::Shrink, Length::Shrink, button_node.size()),
            vec![button_node, icon_node],
        )
    }

    fn draw(
        &self,
        tree: &cosmic::iced_core::widget::Tree,
        renderer: &mut cosmic::Renderer,
        theme: &cosmic::Theme,
        style: &cosmic::iced_core::renderer::Style,
        layout: cosmic::iced_core::Layout<'_>,
        cursor: cosmic::iced_core::mouse::Cursor,
        viewport: &cosmic::iced_core::Rectangle,
    ) {
        let children = &[&self.image_button, &self.icon];
        // draw children in order
        for (i, (layout, child)) in layout.children().zip(children).enumerate() {
            let tree = &tree.children[i];
            child
                .as_widget()
                .draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn size_hint(&self) -> Size<Length> {
        self.size()
    }

    fn tag(&self) -> cosmic::iced_core::widget::tree::Tag {
        cosmic::iced_core::widget::tree::Tag::stateless()
    }

    fn state(&self) -> cosmic::iced_core::widget::tree::State {
        cosmic::iced_core::widget::tree::State::None
    }

    fn operate(
        &self,
        tree: &mut cosmic::iced_core::widget::Tree,
        layout: cosmic::iced_core::Layout<'_>,
        renderer: &cosmic::Renderer,
        operation: &mut dyn cosmic::widget::Operation<
            cosmic::iced_core::widget::OperationOutputWrapper<Msg>,
        >,
    ) {
        let layout = layout.children().collect::<Vec<_>>();
        let children = [&self.image_button, &self.icon];
        for (i, (layout, child)) in layout
            .into_iter()
            .zip(children.into_iter())
            .enumerate()
            .rev()
        {
            let tree = &mut tree.children[i];
            child.as_widget().operate(tree, layout, renderer, operation);
        }
    }

    fn on_event(
        &mut self,
        state: &mut cosmic::iced_core::widget::Tree,
        event: cosmic::iced_core::Event,
        layout: cosmic::iced_core::Layout<'_>,
        cursor: cosmic::iced_core::mouse::Cursor,
        renderer: &cosmic::Renderer,
        clipboard: &mut dyn cosmic::iced_core::Clipboard,
        shell: &mut cosmic::iced_core::Shell<'_, Msg>,
        viewport: &cosmic::iced_core::Rectangle,
    ) -> cosmic::iced_core::event::Status {
        let children = [&mut self.image_button, &mut self.icon];

        let layout = layout.children().collect::<Vec<_>>();
        // draw children in order
        let mut status = cosmic::iced_core::event::Status::Ignored;
        for (i, (layout, child)) in layout
            .into_iter()
            .zip(children.into_iter())
            .enumerate()
            .rev()
        {
            let tree = &mut state.children[i];

            status = child.as_widget_mut().on_event(
                tree,
                event.clone(),
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
            if matches!(status, cosmic::iced_core::event::Status::Captured) {
                return status;
            }
        }
        status
    }

    fn mouse_interaction(
        &self,
        state: &cosmic::iced_core::widget::Tree,
        layout: cosmic::iced_core::Layout<'_>,
        cursor: cosmic::iced_core::mouse::Cursor,
        viewport: &cosmic::iced_core::Rectangle,
        renderer: &cosmic::Renderer,
    ) -> cosmic::iced_core::mouse::Interaction {
        let children = [&self.image_button, &self.icon];
        let layout = layout.children().collect::<Vec<_>>();
        for (i, (layout, child)) in layout
            .into_iter()
            .zip(children.into_iter())
            .enumerate()
            .rev()
        {
            let tree = &state.children[i];
            let interaction = child
                .as_widget()
                .mouse_interaction(tree, layout, cursor, viewport, renderer);
            if cursor.is_over(layout.bounds()) {
                return interaction;
            }
        }
        cosmic::iced_core::mouse::Interaction::Idle
    }

    fn id(&self) -> Option<cosmic::widget::Id> {
        None
    }

    fn set_id(&mut self, _id: cosmic::widget::Id) {}
}

impl<'a, Message> From<WindowImage<'a, Message>> for cosmic::Element<'a, Message>
where
    Message: 'static + Clone,
{
    fn from(w: WindowImage<'a, Message>) -> cosmic::Element<'a, Message> {
        Element::new(w)
    }
}
