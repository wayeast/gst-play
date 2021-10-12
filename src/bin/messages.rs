// Try to wrangle messages, eg those emitted by spectrum or level elements
// Still need to get message data to something in MainLoop.  maybe via channel???

use glib::Value;
use glib::translate::ToGlibPtr;
use gst::prelude::*;
use gst_pbutils::prelude::*;
use gst_sys::GstValueList;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button};
use gtk::glib::{self, Sender, Receiver};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

// const FILESRC: &str = "/home/hustonb/Downloads/7dot1voiced/Nums_5dot1_24_48000.wav";
const FILESRC: &str = "/home/hustonb/Workspace/testfiles/jen_helsby.ogv";

fn main() {
    gst::init().unwrap();
    // Create a new application
    let app = Application::builder()
        .application_id("org.gtk.example")
        .build();

    // Connect to "activate" signal of `app`
    app.connect_activate(build_ui);

    // Run the application
    app.run();
}

fn build_ui(app: &Application) {
    // Create a window and set the title
    let window = ApplicationWindow::builder()
        .application(app)
        .title("My GTK App")
        .build();
    window.set_size_request(980, 780);

    window.present();

    let pipeline = make_pipeline();
    let pipeline = RefCell::new(Some(pipeline));
    app.connect_shutdown(move |_| {
        // Optional, by manually destroying the window here we ensure that
        // the gst element is destroyed when shutting down instead of having to wait
        // for the process to terminate, allowing us to use the leaks tracer.
        unsafe {
            window.destroy();
        }

        // GTK will keep the Application alive for the whole process lifetime.
        // Wrapping the pipeline in a RefCell<Option<_>> and removing it from it here
        // ensures the pipeline is actually destroyed when shutting down, allowing us
        // to use the leaks tracer for example.
        if let Some(pipeline) = pipeline.borrow_mut().take() {
            pipeline
                .set_state(gst::State::Null)
                .expect("Unable to set the pipeline to the `Null` state");
            pipeline.bus().unwrap().remove_watch().unwrap();
        }
    });
}

fn make_pipeline() -> gst::Pipeline {
    let pipeline = gst::Pipeline::new(None);
    let src = gst::ElementFactory::make("filesrc", None).unwrap();
    src.set_property("location", FILESRC).unwrap();
    let decodebin = gst::ElementFactory::make("decodebin", None).unwrap();

    pipeline.add_many(&[&src, &decodebin]).unwrap();
    gst::Element::link_many(&[&src, &decodebin]).unwrap();
    let pipeline_weak = pipeline.downgrade();
    decodebin.connect_pad_added(move |_, src_pad| {
        let pipeline = match pipeline_weak.upgrade() {
            Some(pipeline) => pipeline,
            None => return,
        };
        let (is_audio, is_video) = src_pad
            .current_caps()
            .and_then(|caps| {
                caps.structure(0).map(|s| {
                    let name = s.name();
                    (name.starts_with("audio/"), name.starts_with("video/"))
                })
            })
        .unwrap();

        if is_audio {
            let tee = gst::ElementFactory::make("tee", None).unwrap();
            let spectrum = gst::ElementFactory::make("spectrum", None).unwrap();
            // spectrum.set_property("multi-channel", true).unwrap();
            let level = gst::ElementFactory::make("level", None).unwrap();
            let sink = gst::ElementFactory::make("autoaudiosink", None).unwrap();
            pipeline.add_many(&[&tee, &spectrum, &level, &sink]).unwrap();
            for e in &[&tee, &spectrum, &level, &sink] {
                e.sync_state_with_parent().unwrap();
            }
            let tee_sink = tee.static_pad("sink").unwrap();
            src_pad.link(&tee_sink).unwrap();
            tee.link_pads(None, &spectrum, None).unwrap();
            tee.link_pads(None, &level, None).unwrap();
            tee.link_pads(None, &sink, None).unwrap();
        }
    });

    let bus = pipeline.bus().unwrap();
    bus.add_watch_local(move |_, msg| {
        use gst::MessageView;
        unsafe {
            println!("msg as GstMessage");
            let gst_msg = msg.as_ptr();
            println!("gst_msg: {:?}", gst_msg);
        }
        println!("msg.structure: {:#?}", msg.structure());
        if let Some(structure) = msg.structure() {
            println!("structure name: {}", structure.name());
            println!("structure fields: {:?}", structure.fields().collect::<Vec<&'static str>>());
            match structure.name() {
                "spectrum" => {
                    let vald = structure.value("magnitude").unwrap();
                    println!("spectrum value magnitude: {:?}", vald);
                    let t = vald.type_();
                    println!("magnitude type: {:?}", t);
                    let array = vald.get::<gst::List>().unwrap();
                    // let first = array[0].get::<f64>().unwrap();
                    // println!("first peak: {:?}", first);
                    let magnitudes = array.iter().map(|v| v.get::<f64>().unwrap()).collect::<Vec<_>>();
                    // let val: Value = vald.into();
                    // let stg = val.to_glib_none().0;
                    // println!("something?: {:?}", stg);
                },
                "level" => {
                    let vald = structure.value("peak").unwrap();
                    println!("level value peak: {:?}", vald);
                    let array = vald.get::<glib::ValueArray>().unwrap();
                    let first = array[0].get::<f64>().unwrap();
                    println!("first peak: {:?}", first);
                    // let first = vald[0].get::<f64>();
                    // println!("first: {:?}", first);
                    // let got = structure.get::<Vec<f32>>("peak").unwrap();
                }
                _ => println!("got some other kind of message"),
            }
        }
        println!("msg view: {:?}", msg.view());
        match msg.view() {
            MessageView::Error(err) => {
                println!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                    );
            }
            _ => (),
        };
        println!("msg type: {:?}", msg.type_());

        glib::Continue(true)
    })
    .expect("Failed to add bus watch");
    pipeline.set_state(gst::State::Playing).unwrap();

    pipeline
}
