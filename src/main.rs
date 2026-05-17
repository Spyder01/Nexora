use nexora::buffer_pool::models::BufferStore;
use nexora::storage::storage_manager::StorageManager;
use nexora::storage::page_store_disk::RegularPageStore;
use nexora::storage::error::NexoraStorageError;

fn main() -> Result<(), NexoraStorageError> {
    let page_store = RegularPageStore::create(std::path::Path::new("./test.nex"))?;
    let mut _storage_manager = StorageManager::from_page_store(BufferStore::new(page_store))?;

    _storage_manager.close()?;
    Ok(())
}
