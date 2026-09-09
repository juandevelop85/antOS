//! Asignador de lista enlazada de bloques libres.
//!
//! La idea que arregla el defecto del asignador de puntero: en vez de un solo
//! puntero, una lista de los huecos libres. Liberar deja de ser un contador y
//! pasa a ser «devolver este hueco a la lista», así que la memoria se reutiliza
//! aunque haya asignaciones de larga vida.
//!
//! El truco bonito: la lista **no necesita memoria propia**. Cada nodo se
//! escribe dentro del hueco libre que describe. Un bloque libre es su propia
//! entrada de índice. De ahí que el tamaño mínimo de bloque sea el tamaño de
//! un nodo: por debajo de eso, el hueco no puede describirse a sí mismo.
//!
//! ## Lo que este asignador todavía NO hace
//!
//! No fusiona bloques libres adyacentes. Liberar dos huecos contiguos deja dos
//! entradas de la mitad de tamaño en vez de una grande, así que una carga con
//! tamaños variados acaba fragmentando el heap: queda memoria libre de sobra,
//! pero ningún hueco lo bastante grande. Reutilizar bloques del mismo tamaño
//! —que es lo habitual— sí funciona perfectamente.

use super::{align_up, Locked};
use core::alloc::{GlobalAlloc, Layout};
use core::mem;
use core::ptr;

struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        ListNode { size, next: None }
    }

    /// La dirección del nodo ES la dirección del hueco: viven en el mismo sitio.
    fn start(&self) -> usize {
        self as *const Self as usize
    }

    fn end(&self) -> usize {
        self.start() + self.size
    }
}

pub struct LinkedListAllocator {
    /// Nodo centinela de tamaño 0. Tener una cabeza que siempre existe evita
    /// tratar el caso «lista vacía» como algo especial en cada operación.
    head: ListNode,
    heap_size: usize,
}

impl LinkedListAllocator {
    pub const fn new() -> Self {
        LinkedListAllocator {
            head: ListNode::new(0),
            heap_size: 0,
        }
    }

    /// # Safety
    /// El rango debe estar mapeado y ser de uso exclusivo del heap.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_size = heap_size;
        // El heap entero empieza siendo un único hueco libre.
        unsafe { self.add_free_region(heap_start, heap_size) };
    }

    /// Devuelve un hueco a la lista, escribiendo su nodo dentro de él.
    ///
    /// # Safety
    /// El rango debe estar libre, mapeado, y no volver a usarse hasta que se
    /// asigne de nuevo.
    unsafe fn add_free_region(&mut self, address: usize, size: usize) {
        debug_assert_eq!(align_up(address, mem::align_of::<ListNode>()), address);
        debug_assert!(size >= mem::size_of::<ListNode>());

        let mut node = ListNode::new(size);
        node.next = self.head.next.take();

        let node_ptr = address as *mut ListNode;
        unsafe {
            node_ptr.write(node);
            self.head.next = Some(&mut *node_ptr);
        }
    }

    /// Busca el primer hueco que sirva y lo saca de la lista.
    ///
    /// Es la estrategia «first fit»: rápida, y peor que «best fit» en
    /// fragmentación. Con un asignador que no fusiona, tampoco compensa
    /// afinar más.
    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        let mut current = &mut self.head;

        while let Some(ref mut region) = current.next {
            if let Ok(allocation_start) = Self::allocate_from(region, size, align) {
                // Sacarlo de la lista sin romper el encadenado.
                let next = region.next.take();
                let found = current.next.take().unwrap();
                current.next = next;
                return Some((found, allocation_start));
            }
            current = current.next.as_mut().unwrap();
        }
        None
    }

    /// ¿Cabe la petición en este hueco, respetando la alineación?
    fn allocate_from(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let start = align_up(region.start(), align);
        let end = start.checked_add(size).ok_or(())?;

        if end > region.end() {
            return Err(());
        }

        // Si sobra un trozo, tiene que ser lo bastante grande para poder
        // describirse a sí mismo. Un resto más pequeño sería memoria perdida
        // que ya nadie sabría encontrar.
        let excess = region.end() - end;
        if excess > 0 && excess < mem::size_of::<ListNode>() {
            return Err(());
        }

        Ok(start)
    }

    /// Ajusta la petición para que el bloque pueda albergar un nodo el día que
    /// se libere.
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .expect("alineación imposible")
            .pad_to_align();
        (
            layout.size().max(mem::size_of::<ListNode>()),
            layout.align(),
        )
    }

    pub fn free(&self) -> usize {
        let mut total = 0;
        let mut current = self.head.next.as_ref();
        while let Some(region) = current {
            total += region.size;
            current = region.next.as_ref();
        }
        total
    }

    pub fn used(&self) -> usize {
        self.heap_size - self.free()
    }
}

unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (size, align) = LinkedListAllocator::size_align(layout);
        let mut allocator = self.lock();

        let Some((region, start)) = allocator.find_region(size, align) else {
            return ptr::null_mut();
        };

        let end = start.checked_add(size).expect("desbordamiento");
        let excess = region.end() - end;
        if excess > 0 {
            // Lo que sobra del hueco vuelve a la lista como hueco propio.
            unsafe { allocator.add_free_region(end, excess) };
        }
        start as *mut u8
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (size, _) = LinkedListAllocator::size_align(layout);
        unsafe { self.lock().add_free_region(ptr as usize, size) };
    }
}
