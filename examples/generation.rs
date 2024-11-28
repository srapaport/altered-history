// use std::fs::File;
// use std::path::Path;
// use mmap_rs::Mmap;

// fn main() {
//     let path = Path::new("/home/infres/rapaport/datasets/2024-08-23/topology/depths_backward_dir,rev,rel,snp,ori.bin");
//     let file = File::open(path).unwrap();
//     let mmap: swh_graph::utils::mmap::NumberMmap::<byteorder::BE, u32, Mmap> = swh_graph::utils::mmap::NumberMmap::new(path, 5134253904).expect("couldnt map");
//     // = unsafe { Mmap::map(&file).expect("couldn't mmap")  }.try_into().expect("couldn't cast");
// }

use std::fs::File;
use std::path::Path;
use memmap::Mmap;
use std::mem;

fn main() {
    let path = Path::new("/home/infres/rapaport/datasets/2024-08-23/topology/depths_backward_dir,rev,rel,snp,ori.bin");

    // Open the file
    let file = File::open(&path).expect("Could not open file");

    // Create a memory map
    let mmap = unsafe { Mmap::map(&file).expect("Couldn't mmap") };

    // Access the contents of the file
    let data = mmap.as_ref();
    println!("data.len() = {}", data.len());

    // Ensure the data length is a multiple of the tuple size
    assert!(data.len() % mem::size_of::<u32>() == 0, "File size is not a multiple of tuple size");

    // Interpret the data as an array of u32 values
    let values: &[u32] = unsafe {
        std::slice::from_raw_parts(
            data.as_ptr() as *const u32,
            data.len() / mem::size_of::<u32>(),
        )
    };

    // Print the u32 values
    let mut node_id: usize = 0;
    for value in values {
        if value == 0{
            break;
        }
        node_id += 1;
    }

    let graph = SwhUnidirectionalGraph::new(PathBuf::from("/infres/ir800/rapaport/datasets/2024-08-23-popular-500-python/compressed/graph"))
        .expect("Could not load graph")
        .init_properties()
        .load_properties(|properties| properties.load_maps::<DynMphf>())
        .expect("Could not load maps")
        .load_properties(|properties| properties.load_label_names())
        .expect("Could no load label names")
        .load_labels()
        .expect("Could not load labels")
        .load_properties(SwhGraphProperties::load_strings)
        .expect("Could not load strings");
}