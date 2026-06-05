use std::marker::PhantomData;

#[derive(Clone, Hash)]
pub struct GenPoolId<T> {
    index : u32,
    generation : u32,
    _v : PhantomData<T>,
}

impl<T> GenPoolId<T> {
    pub fn index(&self) -> usize {
        self.index as usize
    }
}

pub struct GenPool<T> {
    items : Vec<Option<T>>,
    generations : Vec<u32>,
}

impl<T> GenPool<T> {
    pub fn new() -> Self {
        Self {
            items : Vec::new(),
            generations : Vec::new(),
        }
    }
    
    pub fn with_capacity(capacity : usize) -> Self {
        Self {
            items : Vec::with_capacity(capacity),
            generations : Vec::with_capacity(capacity)
        }
    }
    

    pub fn alloc(&mut self, val : T) -> GenPoolId<T> {
        let free_index = match self.items.iter_mut().enumerate().find(|v| v.1.is_none()) {
            Some((i, v)) => {
                *v = Some(val);
                self.generations[i] += 1;
                i
            },
            None => {
                let i = self.items.len();
                self.items.push(Some(val));
                self.generations.push(0);
                i
            }
        };
        
        GenPoolId {
            index : free_index as u32,
            generation : self.generations[free_index],
            _v : PhantomData {} 
        }
    }
    

    pub fn take(&mut self, id : GenPoolId<T>) -> T {
       assert!(self.check_valid(&id));
       self.items[id.index()].take().unwrap()
    }
    
    pub fn free(&mut self, id : GenPoolId<T>) {
        let _ = self.take(id);
    }

    pub fn check_valid(&self, id : &GenPoolId<T>) -> bool {
        match self.generations.get(id.index as usize) {
            Some(generation) => *generation == id.generation && self.items[id.index()].is_some(),
            None => false,
        }
    }

    pub fn get(&self, id : &GenPoolId<T>) -> Option<&T> {
        if self.check_valid(id) {
            self.items[id.index()].as_ref()
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, id : &GenPoolId<T>) -> Option<&mut T> {
        if self.check_valid(id) {
            self.items[id.index()].as_mut()
        } else {
            None
        }
    }
}
