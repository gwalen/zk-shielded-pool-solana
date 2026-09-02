/*
 * Because in Quasar we can not have nested arrays like [[u8; 32]; 10]
 * we need to flatten the array to a single dimension array [u8; 32 * 10] (32 bytes per element, 10 elements).
 * And to access easily elements in the flattened array we need this helper functions.
 */ 

/// Read the 32-byte element stored at `index` of a flattened array.
///
/// Panics if `index` is past the end of `array`; every caller derives the index from a
/// tree level or ring-buffer position the program itself bounds, never from instruction data.
pub fn get_array_element(array: &[u8], index: usize) -> [u8; 32] {
    let mut element = [0u8; 32];
    let start_element = index * 32;
    element.copy_from_slice(&array[start_element..start_element + 32]);
    element
}

/// Write `element` into slot `index` of a flattened array
pub fn set_array_element(array: &mut [u8], index: usize, element: &[u8; 32]) {
    let start_element = index * 32;
    array[start_element..start_element + 32].copy_from_slice(element);
}