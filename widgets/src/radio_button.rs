use {
    crate::{
        makepad_derive_widget::*,
        makepad_draw::*,
        widget::*,
        data_binding::DataBinding,
        frame::*,
    }
};

live_design!{
    import makepad_draw::shader::std::*;


    DrawRadioButton = {{DrawRadioButton}} {

        uniform size: 7.0;
        fn pixel(self) -> vec4 {
            let sdf = Sdf2d::viewport(self.pos * self.rect_size)
            match self.radio_type {
                RadioType::Round => {
                    let sz = self.size;
                    let left = sz + 1.;
                    let c = vec2(left + sz, self.rect_size.y * 0.5);
                    sdf.circle(left, c.y, sz);
                    sdf.fill(#2);
                    let isz = sz * 0.5;
                    sdf.circle(left, c.y, isz);
                    sdf.fill(mix(#fff0, #f, self.selected));
                }
                RadioType::Tab => {
                    let sz = self.size;
                    let left = 0.;
                    let c = vec2(left, self.rect_size.y);
                    sdf.rect(
                        -1., 0.,
                        self.rect_size.x + 2.0,
                        self.rect_size.y 
                    );
                    sdf.fill(mix(self.color_inactive, self.color_active, self.selected));
                }
            }
            return sdf.result
        }


    }
    
    RadioButton = {{RadioButton}} {
        label_text: {
            instance hover: 0.0
            instance focus: 0.0
            instance selected: 0.0
            instance color_unselected: #x00000088
            instance color_unselected_hover: #x000000CC
            instance color_selected: #xFFFFFF66
            color: #9
            text_style: {
                font: {
                    //path: d"resources/ibmplexsans-semibold.ttf"
                }
                font_size: 9.5
            }
            fn get_color(self) -> vec4 {
                return mix(
                    mix(
                        self.color_unselected,
                        self.color_unselected_hover,
                        self.hover
                    ),
                    self.color_selected,
                    self.selected
                )
            }
        }
        
        walk: {
            width: Fit,
            height: Fit
        }

        label_walk: {
            margin: {top: 4.5, bottom: 4.5, left: 8, right: 8}
            width: Fit,
            height: Fit,
        }
        
        radio_button: {
            instance color_active: #00000000
            instance color_inactive: #x99EEFF
        }
        
        label_align: {
            y: 0.0
        }
        
        state: {
            hover = {
                default: off
                off = {
                    from: {all: Forward {duration: 0.15}}
                    apply: {
                        radio_button: {hover: 0.0}
                        label_text: {hover: 0.0}
                    }
                }
                on = {
                    from: {all: Snap}
                    apply: {
                        radio_button: {hover: 1.0}
                        label_text: {hover: 1.0}
                    }
                }
            }
            focus = {
                default: off
                off = {
                    from: {all: Forward {duration: 0.0}}
                    apply: {
                        radio_button: {focus: 0.0}
                        label_text: {focus: 0.0}
                    }
                }
                on = {
                    from: {all: Snap}
                    apply: {
                        radio_button: {focus: 1.0}
                        label_text: {focus: 1.0}
                    }
                }
            }
            selected = {
                default: off
                off = {
                    from: {all: Forward {duration: 0.0}}
                    apply: {
                        radio_button: {selected: 0.0}
                        label_text: {selected: 0.0}
                    }
                }
                on = {
                    cursor: Arrow,
                    from: {all: Forward {duration: 0.0}}
                    apply: {
                        radio_button: {selected: 1.0}
                        label_text: {selected: 1.0}
                    }
                }
            }
        }
    }
}

#[derive(Live, LiveHook)]
#[repr(C)]
pub struct DrawRadioButton {
    draw_super: DrawQuad,
    radio_type: RadioType,
    hover: f32,
    focus: f32,
    selected: f32
}


#[derive(Live, LiveHook)]
#[repr(u32)]
pub enum RadioType {
    #[pick] Round = shader_enum(1),
    Tab = shader_enum(2),
}

#[derive(Live, LiveHook)]
#[live_design_fn(widget_factory!(RadioButton))]
pub struct RadioButton {
    radio_button: DrawRadioButton,
    
    walk: Walk,
    
    value: LiveValue,
    
    layout: Layout,
    state: State,
    
    label_walk: Walk,
    label_align: Align,
    label_text: DrawText,
    label: String,
    
    bind: String,
}

#[derive(Clone, WidgetAction)]
pub enum RadioButtonAction {
    Clicked,
    None
}


impl RadioButton {
    
    pub fn handle_event_fn(&mut self, cx: &mut Cx, event: &Event, dispatch_action: &mut dyn FnMut(&mut Cx, RadioButtonAction)) {
        self.state_handle_event(cx, event);
        
        match event.hits(cx, self.radio_button.area()) {
            Hit::FingerHoverIn(_) => {
                cx.set_cursor(MouseCursor::Hand);
                self.animate_state(cx, id!(hover.on));
            }
            Hit::FingerHoverOut(_) => {
                cx.set_cursor(MouseCursor::Arrow);
                self.animate_state(cx, id!(hover.off));
            },
            Hit::FingerDown(_fe) => {
                if self.state.is_in_state(cx, id!(selected.off)) {
                    self.animate_state(cx, id!(selected.on));
                    dispatch_action(cx, RadioButtonAction::Clicked);
                }
            },
            Hit::FingerUp(_fe) => {
                
            }
            Hit::FingerMove(_fe) => {
                
            }
            _ => ()
        }
    }
    
    pub fn draw_walk(&mut self, cx: &mut Cx2d, walk: Walk) {
        self.radio_button.begin(cx, walk, self.layout);
        self.label_text.draw_walk(cx, self.label_walk, self.label_align, &self.label);
        self.radio_button.end(cx);
    }
}

impl Widget for RadioButton {
    fn widget_uid(&self) -> WidgetUid {return WidgetUid(self as *const _ as u64)}
    
    fn bind_to(&mut self, _cx: &mut Cx, _db: &mut DataBinding, _act: &WidgetActions, _path: &[LiveId]) {
    }
    
    fn redraw(&mut self, cx: &mut Cx) {
        self.radio_button.redraw(cx);
    }
    
    fn handle_widget_event_fn(&mut self, cx: &mut Cx, event: &Event, dispatch_action: &mut dyn FnMut(&mut Cx, WidgetActionItem)) {
        let uid = self.widget_uid();
        self.handle_event_fn(cx, event, &mut | cx, action | {
            dispatch_action(cx, WidgetActionItem::new(action.into(), uid))
        });
    }
    
    fn get_walk(&self) -> Walk {self.walk}
    
    fn draw_widget(&mut self, cx: &mut Cx2d, walk: Walk) -> WidgetDraw {
        self.draw_walk(cx, walk);
        WidgetDraw::done()
    }
}

#[derive(Clone, PartialEq, WidgetRef)]
pub struct RadioButtonRef(WidgetRef);

impl RadioButtonRef{
    fn unselect(&self, cx:&mut Cx){
        if let Some(mut inner) = self.inner_mut(){
            inner.animate_state(cx, id!(selected.off));
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct RadioGroupRef<const N: usize>([RadioButtonRef; N]);

pub trait RadioGroupFrameRefExt {
    fn get_radio_group<'a, const N: usize>(&self, paths: &[&[LiveId]; N]) -> RadioGroupRef<N>;
}

impl<const N: usize> RadioGroupRef<N>{
    
    pub fn selected(&self, cx: &mut Cx, actions: &WidgetActions)->Option<usize>{
        for action in actions{
            match action.action() {
                RadioButtonAction::Clicked => if let Some(index) = self.0.iter().position(|v| v.widget_uid() == action.widget_uid){
                    for i in 0..self.0.len(){
                        if i != index{
                            self.0[i].unselect(cx);
                        }
                    }
                    return Some(index);
                }
                _ => ()
            }
        }
        None
    }
    
    pub fn selected_to_visible(&self, cx: &mut Cx, ui:&FrameRef, actions: &WidgetActions, paths:&[&[LiveId];N] ) {
        // find a widget action that is in our radiogroup
        if let Some(index) = self.selected(cx, actions){
            // ok now we set visible
            for (i,path) in paths.iter().enumerate(){
                let mut widget = ui.get_widget(path);
                widget.apply_over(cx, live!{visible:(i == index)});
                widget.redraw(cx);
            }
        }
    }
}

impl RadioGroupFrameRefExt for FrameRef{
    fn get_radio_group<const N: usize>(&self, paths: &[&[LiveId]; N]) -> RadioGroupRef<N> {
        // lets return a radio group
        RadioGroupRef(core::array::from_fn( | i | {
            RadioButtonRef(self.get_widget(paths[i]))
        }))
    }
}