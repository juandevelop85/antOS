//! Asignador de lista enlazada de bloques libres con coalescencia contigua.
//!
//! La idea que arregla el defecto del asignador de puntero: en vez de un solo
//! puntero, una lista ordenada por dirección física de los huecos libres.
//! Liberar devuelve el hueco a la lista y fusiona (coalesce) bloques contiguos
//! a la izquierda, derecha o ambos («sándwich»), evitando la fragmentación
//! externa sin overhead de memoria externa.
//!
//! El truco bonito: la lista **no necesita memoria propia**. Cada nodo se
//! escribe dentro del hueco libre que describe. Un bloque libre es su propia
//! entrada de índice. De ahí que el tamaño mínimo de bloque sea el tamaño de
//! un nodo: por debajo de eso, el hueco no puede describirse a sí mismo.
//!
//! Mantiene la contabilidad de memoria asignada en `O(1)` (`allocated_bytes`),
//! permitiendo consultas instantáneas de memoria usada y libre sin recorrer la lista.

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
    allocated_bytes: usize,
}

impl LinkedListAllocator {
    pub const fn new() -> Self {
        LinkedListAllocator {
            head: ListNode::new(0),
            heap_size: 0,
            allocated_bytes: 0,
        }
    }

    /// # Safety
    /// El rango debe estar mapeado y ser de uso exclusivo del heap.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_size = heap_size;
        self.allocated_bytes = 0;
        // El heap entero empieza siendo un único hueco libre.
        unsafe { self.add_free_region(heap_start, heap_size) };
    }

    /// Devuelve un hueco a la lista insertándolo en orden de dirección física
    /// y coalesciendo con los bloques inmediatamente adyacentes a la izquierda,
    /// a la derecha, o ambos (fusión tipo «sándwich»).
    ///
    /// # Safety
    /// El rango debe estar libre, mapeado, y no volver a usarse hasta que se
    /// asigne de nuevo.
    unsafe fn add_free_region(&mut self, address: usize, mut size: usize) {
        debug_assert_eq!(align_up(address, mem::align_of::<ListNode>()), address);
        debug_assert!(size >= mem::size_of::<ListNode>());

        // 1. Recorrer la lista para encontrar el predecesor donde insertar en orden de dirección.
        let mut prev = &mut self.head as *mut ListNode;
        unsafe {
            while let Some(ref mut next_node) = (*prev).next {
                if next_node.start() > address {
                    break;
                }
                prev = &mut **next_node as *mut ListNode;
            }

            // 2. Extraer el nodo siguiente (a la derecha de la nueva región).
            let mut next_opt = (*prev).next.take();

            // 3. Fusión hacia la derecha (Right Coalescing):
            // Si el final de la nueva región coincide exactamente con el inicio de `next`.
            if let Some(ref mut next_node) = next_opt {
                if address + size == next_node.start() {
                    size += next_node.size;
                    next_opt = next_node.next.take();
                }
            }

            // 4. Fusión hacia la izquierda (Left Coalescing):
            // Si `prev` no es el centinela `head` y el final de `prev` coincide con `address`.
            if prev != &mut self.head as *mut ListNode && (*prev).end() == address {
                (*prev).size += size;
                (*prev).next = next_opt;
            } else {
                // No hay fusión a la izquierda: se escribe el nuevo nodo en `address`.
                let node_ptr = address as *mut ListNode;
                let mut node = ListNode::new(size);
                node.next = next_opt;
                node_ptr.write(node);
                (*prev).next = Some(&mut *node_ptr);
            }
        }
    }

    /// Busca el primer hueco que sirva y lo saca de la lista («first fit»).
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

        // Si sobra un trozo al final, tiene que ser lo bastante grande para poder
        // describirse a sí mismo como nodo. Un resto más pequeño sería memoria perdida.
        let excess = region.end() - end;
        if excess > 0 && excess < mem::size_of::<ListNode>() {
            return Err(());
        }

        Ok(start)
    }

    /// Ajusta la petición para que el bloque pueda albergar un nodo el día que
    /// se libere.
    pub fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .expect("alineación imposible")
            .pad_to_align();
        (
            layout.size().max(mem::size_of::<ListNode>()),
            layout.align(),
        )
    }

    /// Asigna memoria respetando el layout solicitado y actualiza la contabilidad en O(1).
    pub fn allocate_layout(&mut self, layout: Layout) -> *mut u8 {
        let (size, align) = Self::size_align(layout);

        let Some((region, start)) = self.find_region(size, align) else {
            return ptr::null_mut();
        };

        let end = start.checked_add(size).expect("desbordamiento");
        let excess = region.end() - end;
        if excess > 0 {
            // Lo que sobra del hueco vuelve a la lista como hueco propio.
            unsafe { self.add_free_region(end, excess) };
        }

        self.allocated_bytes = self.allocated_bytes.saturating_add(size);
        start as *mut u8
    }

    /// Libera memoria previamente asignada, insertándola ordenadamente y coalesciendo.
    ///
    /// # Safety
    /// `ptr` debe haber sido obtenido mediante este asignador con el mismo `layout`.
    pub unsafe fn deallocate_layout(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, _) = Self::size_align(layout);
        self.allocated_bytes = self.allocated_bytes.saturating_sub(size);
        unsafe { self.add_free_region(ptr as usize, size) };
    }

    /// Memoria libre en bytes calculada en O(1).
    pub fn free(&self) -> usize {
        self.heap_size.saturating_sub(self.allocated_bytes)
    }

    /// Memoria actualmente en uso en bytes en O(1).
    pub fn used(&self) -> usize {
        self.allocated_bytes
    }

    /// Retorna el conteo actual de nodos en la lista libre (útil para verificar coalescencia).
    pub fn free_node_count(&self) -> usize {
        let mut count = 0;
        let mut current = self.head.next.as_ref();
        while let Some(region) = current {
            count += 1;
            current = region.next.as_ref();
        }
        count
    }

    /// Suma directa de los tamaños de los bloques en la lista de libres (verificación O(N)).
    pub fn sum_free_bytes(&self) -> usize {
        let mut total = 0;
        let mut current = self.head.next.as_ref();
        while let Some(region) = current {
            total += region.size;
            current = region.next.as_ref();
        }
        total
    }
}

unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.lock().allocate_layout(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.lock().deallocate_layout(ptr, layout) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::alloc::Layout;

    #[test_case]
    fn test_linked_list_coalesce_right() {
        let mut buffer = [0u8; 1024];
        let mut allocator = LinkedListAllocator::new();
        unsafe {
            allocator.init(buffer.as_mut_ptr() as usize, 1024);
        }
        assert_eq!(allocator.free_node_count(), 1);

        let layout = Layout::from_size_align(64, 8).unwrap();
        let p1 = allocator.allocate_layout(layout);
        let p2 = allocator.allocate_layout(layout);
        assert!(!p1.is_null());
        assert!(!p2.is_null());

        // Al liberar p1 primero y luego p2 contiguo a su derecha, deben coalescerse
        unsafe {
            allocator.deallocate_layout(p1, layout);
            // p1 está libre; el resto libre está después de p2 (2 huecos)
            assert_eq!(allocator.free_node_count(), 2);
            allocator.deallocate_layout(p2, layout);
            // p2 se coalesció a la izquierda con p1 y a la derecha con el resto del buffer (1 hueco total)
            assert_eq!(allocator.free_node_count(), 1);
        }
    }

    #[test_case]
    fn test_linked_list_coalesce_left() {
        let mut buffer = [0u8; 1024];
        let mut allocator = LinkedListAllocator::new();
        unsafe {
            allocator.init(buffer.as_mut_ptr() as usize, 1024);
        }

        let layout = Layout::from_size_align(64, 8).unwrap();
        let p1 = allocator.allocate_layout(layout);
        let p2 = allocator.allocate_layout(layout);
        let p3 = allocator.allocate_layout(layout);

        // Liberamos p3: se fusiona a la derecha con el resto del buffer
        unsafe {
            allocator.deallocate_layout(p3, layout);
            let count_before = allocator.free_node_count();
            // Liberamos p2: adyacente a la izquierda de [p3 + resto]
            allocator.deallocate_layout(p2, layout);
            // Debe haber fusionado p2 con el bloque a su derecha sin crear nuevos nodos sueltos
            assert_eq!(allocator.free_node_count(), count_before);
            allocator.deallocate_layout(p1, layout);
            assert_eq!(allocator.free_node_count(), 1);
        }
    }

    #[test_case]
    fn test_linked_list_coalesce_sandwich() {
        let mut buffer = [0u8; 2048];
        let mut allocator = LinkedListAllocator::new();
        unsafe {
            allocator.init(buffer.as_mut_ptr() as usize, 2048);
        }

        let layout = Layout::from_size_align(128, 16).unwrap();
        let p1 = allocator.allocate_layout(layout);
        let p2 = allocator.allocate_layout(layout);
        let p3 = allocator.allocate_layout(layout);
        let p4 = allocator.allocate_layout(layout);

        // Liberamos p1 y p3 dejando p2 en el medio ocupado
        unsafe {
            allocator.deallocate_layout(p1, layout);
            allocator.deallocate_layout(p3, layout);
            // Ahora hay 3 huecos libres: p1, p3 y el resto tras p4
            assert_eq!(allocator.free_node_count(), 3);

            // Liberamos p2 (sándwich entre p1 y p3): debe fusionar p1, p2 y p3 en 1 solo bloque
            allocator.deallocate_layout(p2, layout);
            // Quedan 2 huecos libres: [p1+p2+p3] y el resto tras p4
            assert_eq!(allocator.free_node_count(), 2);

            allocator.deallocate_layout(p4, layout);
            // Al liberar p4, todo vuelve a ser 1 único bloque continuo
            assert_eq!(allocator.free_node_count(), 1);
            assert_eq!(allocator.sum_free_bytes(), 2048);
        }
    }

    #[test_case]
    fn test_linked_list_alloc_large_after_coalesce() {
        let mut buffer = [0u8; 2048];
        let mut allocator = LinkedListAllocator::new();
        unsafe {
            allocator.init(buffer.as_mut_ptr() as usize, 2048);
        }

        // Asignamos 4 bloques de 256 bytes (ocupan 1024 bytes)
        let layout_small = Layout::from_size_align(256, 8).unwrap();
        let p1 = allocator.allocate_layout(layout_small);
        let p2 = allocator.allocate_layout(layout_small);
        let p3 = allocator.allocate_layout(layout_small);
        let p4 = allocator.allocate_layout(layout_small);

        // Liberamos los 4 bloques
        unsafe {
            allocator.deallocate_layout(p1, layout_small);
            allocator.deallocate_layout(p2, layout_small);
            allocator.deallocate_layout(p3, layout_small);
            allocator.deallocate_layout(p4, layout_small);
        }

        // Si la coalescencia funciona, ahora podemos pedir un bloque contiguo de 1024 bytes
        let layout_large = Layout::from_size_align(1024, 8).unwrap();
        let big_ptr = allocator.allocate_layout(layout_large);
        assert!(!big_ptr.is_null());

        unsafe {
            allocator.deallocate_layout(big_ptr, layout_large);
        }
        assert_eq!(allocator.free_node_count(), 1);
    }

    #[test_case]
    fn test_linked_list_constant_time_metrics() {
        let mut buffer = [0u8; 4096];
        let mut allocator = LinkedListAllocator::new();
        unsafe {
            allocator.init(buffer.as_mut_ptr() as usize, 4096);
        }

        assert_eq!(allocator.used(), 0);
        assert_eq!(allocator.free(), 4096);

        let layout = Layout::from_size_align(128, 8).unwrap();
        let p1 = allocator.allocate_layout(layout);
        assert!(!p1.is_null());

        let (sz, _) = LinkedListAllocator::size_align(layout);
        assert_eq!(allocator.used(), sz);
        assert_eq!(allocator.free(), 4096 - sz);
        assert_eq!(allocator.free(), allocator.sum_free_bytes());

        unsafe {
            allocator.deallocate_layout(p1, layout);
        }

        assert_eq!(allocator.used(), 0);
        assert_eq!(allocator.free(), 4096);
        assert_eq!(allocator.free(), allocator.sum_free_bytes());
    }

    #[test_case]
    fn test_linked_list_stress_bounded_fragmentation() {
        let mut buffer = [0u8; 8192];
        let mut allocator = LinkedListAllocator::new();
        unsafe {
            allocator.init(buffer.as_mut_ptr() as usize, 8192);
        }

        let layout_a = Layout::from_size_align(64, 8).unwrap();
        let layout_b = Layout::from_size_align(128, 8).unwrap();

        let mut ptrs_a = [ptr::null_mut(); 8];
        let mut ptrs_b = [ptr::null_mut(); 8];

        for _ in 0..10 {
            for i in 0..8 {
                ptrs_a[i] = allocator.allocate_layout(layout_a);
                ptrs_b[i] = allocator.allocate_layout(layout_b);
            }

            // Liberar en orden entrelazado / reverso
            for i in (0..8).rev() {
                unsafe {
                    allocator.deallocate_layout(ptrs_a[i], layout_a);
                }
            }
            for i in 0..8 {
                unsafe {
                    allocator.deallocate_layout(ptrs_b[i], layout_b);
                }
            }

            // Al final de cada ciclo todas las liberaciones coalescen en 1 solo bloque
            assert_eq!(allocator.free_node_count(), 1);
            assert_eq!(allocator.used(), 0);
            assert_eq!(allocator.free(), 8192);
        }
    }
}

