mod crawller;
use crawller::crawller_main;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    crawller_main()
}