fn main() {
    println!("cargo:rerun-if-changed=assets/notas.ico");

    // GPUI carga el ícono de la ventana desde el recurso #1 del ejecutable,
    // que es el que `set_icon` incrusta. Así lo usan también Explorer y la barra de tareas.
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/notas.ico")
            .set("FileDescription", "Notas")
            .set("ProductName", "Notas");
        res.compile().expect("no se pudo incrustar el ícono");
    }
}
