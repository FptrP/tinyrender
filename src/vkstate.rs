use std::sync::Arc;

use vulkano::{
    VulkanLibrary, device::{Device, DeviceCreateInfo, DeviceExtensions, Queue, QueueCreateInfo, QueueFlags, physical::PhysicalDevice}, image::Image, instance::{Instance, InstanceCreateInfo}, swapchain::{Surface, Swapchain, SwapchainCreateInfo}
};

use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};

pub struct State {
    pub instance : Arc<Instance>,
    pub physical_device : Arc<PhysicalDevice>,
    pub surface : Option<Arc<Surface>>,
    pub device : Arc<Device>,
    pub main_queue : Arc<Queue>,
    pub main_queue_family : u32,

    pub swapchain : Option<Arc<Swapchain>>,
    pub backbuffers : Vec<Arc<Image>>,
}

//type WindowType = Arc<impl HasDisplayHandle + HasWindowHandle + Send + Sync>;

impl State {
    
    pub fn new_for_rendering(window : Arc<impl HasWindowHandle + HasDisplayHandle + Send + Sync + 'static>, def_resolution : [u32; 2]) -> State 
    {
        let lib = VulkanLibrary::new().unwrap();
    
        let mut instance_info = InstanceCreateInfo::application_from_cargo_toml();
        instance_info.enabled_extensions = Surface::required_extensions(&window).unwrap();

        let instance = Instance::new(lib, instance_info).unwrap();
        
        let surface = Surface::from_window(instance.clone(), window.clone()).unwrap();

    
        let phys_device = instance.enumerate_physical_devices().unwrap().next().unwrap();
        println!("[vkstate] : picked physical device {}", phys_device.properties().device_name);
        
        let (queue_family, _) = phys_device.queue_family_properties().iter().enumerate().find(|(index, info)| {
            let present_support = phys_device.surface_support(*index as u32, &surface).unwrap();
            let ops_support = info.queue_flags.contains(QueueFlags::GRAPHICS); 
            present_support && ops_support
        }).unwrap();

        println!("[vkstate] : picked physical device queueu family {}", queue_family);        
        //phys_device.surface_support(queue_family_index, surface) 
        
        let mut device_extensions = DeviceExtensions::default();
        device_extensions.khr_swapchain = true;

        let device_info = DeviceCreateInfo {
            enabled_extensions : device_extensions,
            queue_create_infos : vec![
                QueueCreateInfo {
                    queue_family_index : queue_family as u32,
                        queues : vec![1f32],
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        
        let (device, mut queues) = Device::new(phys_device.clone(), device_info).unwrap();
        let main_queue = queues.next().unwrap();
        
        let surface_caps = phys_device.surface_capabilities(&surface, Default::default()).unwrap();
        let resolution = def_resolution;
        println!("[vkstate] creating surface for window {}x{}", resolution[0], resolution[1]);

        let image_format =  phys_device
            .surface_formats(&surface, Default::default())
            .unwrap()[0]
            .0;

        let swapchain_info = SwapchainCreateInfo {
            min_image_count : surface_caps.min_image_count + 1,
            image_format,
            image_extent : resolution, 
            image_usage : surface_caps.supported_usage_flags,
            ..Default::default()
        };
        
        let (swapchain, images) = Swapchain::new(device.clone(), surface.clone(), swapchain_info).unwrap();

        State {
            instance,
            physical_device : phys_device,
            surface : Some(surface),
            device,
            main_queue,
            main_queue_family : queue_family as u32,
            swapchain : Some(swapchain),
            backbuffers : images
        }
    }

}



