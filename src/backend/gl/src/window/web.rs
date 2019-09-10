use crate::{conv, device::Device, native, Backend as B, GlContainer, PhysicalDevice, QueueFamily, Starc};
use arrayvec::ArrayVec;
use glow::Context as _;
use hal::{adapter::Adapter, format as f, image, window};
use std::iter;
use web_sys::{WebGl2RenderingContext, HtmlCanvasElement};
use wasm_bindgen::JsCast;

#[derive(Clone, Debug)]
struct PixelFormat {
    color_bits: u32,
    alpha_bits: u32,
    srgb: bool,
    double_buffer: bool,
    multisampling: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct Instance {
    context: Starc<WebGl2RenderingContext>,
    canvas: Starc<HtmlCanvasElement>,
}

impl Instance {
    pub fn create(_name: &str, _version: u32) -> Result<Self, hal::UnsupportedBackend> {
        let document = web_sys::window()
            .and_then(|win| win.document())
            .expect("Cannot get document");
        let canvas = document
            .create_element("canvas")
            .expect("Cannot create canvas")
            .dyn_into::<HtmlCanvasElement>()
            .expect("Cannot get canvas element");
        let context_options = js_sys::Object::new();
        js_sys::Reflect::set(
            &context_options,
            &"antialias".into(),
            &wasm_bindgen::JsValue::FALSE,
        ).expect("Cannot create context options");
        let context = canvas
            .get_context_with_context_options("webgl2", &context_options)
            .expect("Cannot create WebGL2 context")
            .and_then(|context| context.dyn_into::<WebGl2RenderingContext>().ok())
            .expect("Cannot convert into WebGL2 context");
        Ok(Instance {
            context: Starc::new(context),
            canvas: Starc::new(canvas),
        })
    }

    pub fn create_surface_with_element(&self) -> (Surface, HtmlCanvasElement) {
        (
            Surface {
                canvas: Starc::clone(&self.canvas),
                swapchain: None,
                renderbuffer: None,
            },
            (*self.canvas).clone(),
        )
    }

}

impl hal::Instance for Instance {
    type Backend = B;
    fn enumerate_adapters(&self) -> Vec<Adapter<B>> {
        let adapter = PhysicalDevice::new_adapter((), GlContainer::from_webgl2_context((*self.context).clone())); // TODO: Move to `self` like native/window
        vec![adapter]
    }
}

#[derive(Clone, Debug)]
pub struct Swapchain {
    pub(crate) extent: window::Extent2D,
    pub(crate) fbos: ArrayVec<[native::RawFrameBuffer; 3]>,
}

impl window::Swapchain<B> for Swapchain {
    unsafe fn acquire_image(
        &mut self,
        _timeout_ns: u64,
        _semaphore: Option<&native::Semaphore>,
        _fence: Option<&native::Fence>,
    ) -> Result<(window::SwapImageIndex, Option<window::Suboptimal>), window::AcquireError> {
        // TODO: sync
        Ok((0, None))
    }
}

#[derive(Clone, Debug)]
pub struct Surface {
    canvas: Starc<web_sys::HtmlCanvasElement>,
    pub(crate) swapchain: Option<Swapchain>,
    renderbuffer: Option<native::Renderbuffer>,
}

impl Surface {
    fn swapchain_formats(&self) -> Vec<f::Format> {
        vec![f::Format::Rgba8Unorm, f::Format::Bgra8Unorm]
    }
}

impl window::Surface<B> for Surface {
    fn compatibility(
        &self,
        _: &PhysicalDevice,
    ) -> (
        window::SurfaceCapabilities,
        Option<Vec<f::Format>>,
        Vec<window::PresentMode>,
    ) {
        let extent = hal::window::Extent2D {
            width: self.canvas.width(),
            height: self.canvas.height(),
        };

        let caps = window::SurfaceCapabilities {
            image_count: 2 ..= 2,
            current_extent: Some(extent),
            extents: extent ..= extent,
            max_image_layers: 1,
            usage: image::Usage::COLOR_ATTACHMENT | image::Usage::TRANSFER_SRC,
            composite_alpha: window::CompositeAlpha::OPAQUE, //TODO
        };
        let present_modes = vec![
            window::PresentMode::Fifo, //TODO
        ];

        (caps, Some(self.swapchain_formats()), present_modes)
    }

    fn supports_queue_family(&self, _: &QueueFamily) -> bool {
        true
    }
}

impl window::PresentationSurface<B> for Surface {
    type SwapchainImage = native::ImageView;

    unsafe fn configure_swapchain(
        &mut self,
        device: &Device,
        config: window::SwapchainConfig,
    ) -> Result<(), window::CreationError> {
        let gl = &device.share.context;

        if let Some(old) = self.swapchain.take() {
            for fbo in old.fbos {
                gl.delete_framebuffer(fbo);
            }
        }

        if self.renderbuffer.is_none() {
            self.renderbuffer = Some(gl.create_renderbuffer().unwrap());
        }

        let desc = conv::describe_format(config.format).unwrap();
        gl.bind_renderbuffer(glow::RENDERBUFFER, self.renderbuffer);
        gl.renderbuffer_storage(
            glow::RENDERBUFFER,
            desc.tex_internal,
            config.extent.width as i32,
            config.extent.height as i32,
        );

        let fbo = gl.create_framebuffer().unwrap();
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
        gl.framebuffer_renderbuffer(
            glow::READ_FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::RENDERBUFFER,
            self.renderbuffer,
        );
        self.swapchain = Some(Swapchain {
            extent: config.extent,
            fbos: iter::once(fbo).collect(),
        });

        Ok(())
    }

    unsafe fn unconfigure_swapchain(&mut self, device: &Device) {
        let gl = &device.share.context;
        if let Some(old) = self.swapchain.take() {
            for fbo in old.fbos {
                gl.delete_framebuffer(fbo);
            }
        }
        if let Some(rbo) = self.renderbuffer.take() {
            gl.delete_renderbuffer(rbo);
        }
    }

    unsafe fn acquire_image(
        &mut self,
        _timeout_ns: u64,
    ) -> Result<(Self::SwapchainImage, Option<window::Suboptimal>), window::AcquireError> {
        let image = native::ImageView::Renderbuffer(self.renderbuffer.unwrap());
        Ok((image, None))
    }
}
