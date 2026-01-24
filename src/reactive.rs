//! eframe 0.33.3 can leave ControlFlow::Poll set after its repaint queue empties.
//! Keep queued redraws and timed repaint deadlines, but wait once the event batch ends.
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    window::WindowId,
};

pub struct Reactive<T>(pub T);
impl<T: ApplicationHandler<eframe::UserEvent>> ApplicationHandler<eframe::UserEvent>
    for Reactive<T>
{
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.0.new_events(event_loop, cause);
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.0.resumed(event_loop);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.0.window_event(event_loop, id, event);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: eframe::UserEvent) {
        self.0.user_event(event_loop, event);
    }
    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        self.0.device_event(event_loop, id, event);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.0.about_to_wait(event_loop);
        if matches!(event_loop.control_flow(), ControlFlow::Poll) {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        self.0.suspended(event_loop);
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.0.exiting(event_loop);
    }
    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.0.memory_warning(event_loop);
    }
}
