use nexora::buffer_pool::models::BufferStore;
use nexora::storage::storage_manager::StorageManager;
use nexora::storage::page_store_disk::RegularPageStore;
use nexora::storage::error::NexoraStorageError;

fn main() -> Result<(), NexoraStorageError> {
    let page_store = RegularPageStore::create(std::path::Path::new("./test.nex"))?;
    let buffer_store = BufferStore::new_boxed(page_store);
    let mut _storage_manager = StorageManager::from_page_store(*buffer_store)?;

    _storage_manager.close()?;
    Ok(())
}
