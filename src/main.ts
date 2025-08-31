import wasmInit, { wasm_main } from '../pkg/pngsort.js';

interface SortConfig {
    descending: boolean;
    sort_range: 'Row' | 'Column' | 'RowMajor' | 'ColumnMajor';
    sort_mode?: 'TiedBySum' | 'TiedByOrder' | 'Untied';
    sort_channel: string[];
}

class PngSorter {
    private isInitialized = false;

    async init(): Promise<void> {
        try {
            // Initialize WASM module
            await wasmInit();
            this.isInitialized = true;
            console.log('WASM module initialized successfully');
        } catch (error) {
            console.error('Failed to initialize WASM module:', error);
            throw new Error('Failed to initialize PNG sorter');
        }
    }

    async processImage(imageFile: File, config: SortConfig): Promise<Uint8Array> {
        if (!this.isInitialized) {
            throw new Error('WASM module not initialized');
        }

        try {
            // Convert image file to Uint8Array
            const arrayBuffer = await imageFile.arrayBuffer();
            const inputArray = new Uint8Array(arrayBuffer);

            // Convert config to JSON string
            const configStr = JSON.stringify(config);

            // Process the image using WASM
            const result = wasm_main(configStr, inputArray);

            return result;
        } catch (error) {
            console.error('Error processing image:', error);
            throw new Error('Failed to process image');
        }
    }
}

class PngSortApp {
    private pngSorter: PngSorter;
    private currentImageFile: File | null = null;
    private resultBlob: Blob | null = null;

    // DOM elements
    private imageInput!: HTMLInputElement;
    private sortRange!: HTMLSelectElement;
    private sortMode!: HTMLSelectElement;
    private channelList!: HTMLElement;
    private descending!: HTMLInputElement;
    private processBtn!: HTMLButtonElement;
    private resultContainer!: HTMLElement;
    private downloadBtn!: HTMLAnchorElement;
    private messageContainer!: HTMLElement;
    private hintText!: HTMLElement;

    constructor() {
        this.pngSorter = new PngSorter();
        this.initializeElements();
        this.setupEventListeners();
        this.updateChannelOrderHint();
        this.initializeWasm();
    }

    private initializeElements(): void {
        this.imageInput = document.getElementById('imageInput') as HTMLInputElement;
        this.sortRange = document.getElementById('sortRange') as HTMLSelectElement;
        this.sortMode = document.getElementById('sortMode') as HTMLSelectElement;
        this.channelList = document.getElementById('channelList') as HTMLElement;
        this.descending = document.getElementById('descending') as HTMLInputElement;
        this.processBtn = document.getElementById('processBtn') as HTMLButtonElement;
        this.resultContainer = document.getElementById('resultContainer') as HTMLElement;
        this.downloadBtn = document.getElementById('downloadBtn') as HTMLAnchorElement;
        this.messageContainer = document.getElementById('messageContainer') as HTMLElement;
        this.hintText = document.getElementById('hintText') as HTMLElement;
    }

    private setupEventListeners(): void {
        this.imageInput.addEventListener('change', this.handleImageSelect.bind(this));
        this.processBtn.addEventListener('click', this.handleProcessImage.bind(this));
        this.sortMode.addEventListener('change', this.updateChannelOrderHint.bind(this));
        this.setupDragAndDrop();
    }

    private setupDragAndDrop(): void {
        const channelItems = this.channelList.querySelectorAll('.channel-item');

        channelItems.forEach(item => {
            item.addEventListener('dragstart', (e) => this.handleDragStart(e as DragEvent));
            item.addEventListener('dragover', (e) => this.handleDragOver(e as DragEvent));
            item.addEventListener('dragenter', (e) => this.handleDragEnter(e as DragEvent));
            item.addEventListener('dragleave', (e) => this.handleDragLeave(e as DragEvent));
            item.addEventListener('drop', (e) => this.handleDrop(e as DragEvent));
            item.addEventListener('dragend', () => this.handleDragEnd());
        });
    }

    private draggedElement: HTMLElement | null = null;

    private handleDragStart(e: DragEvent): void {
        this.draggedElement = e.target as HTMLElement;
        this.draggedElement.classList.add('dragging');
        if (e.dataTransfer) {
            e.dataTransfer.effectAllowed = 'move';
        }
    }

    private handleDragOver(e: DragEvent): void {
        e.preventDefault();
        if (e.dataTransfer) {
            e.dataTransfer.dropEffect = 'move';
        }
    }

    private handleDragEnter(e: DragEvent): void {
        e.preventDefault();
        const target = e.target as HTMLElement;
        const channelItem = target.closest('.channel-item') as HTMLElement;
        if (channelItem && channelItem !== this.draggedElement) {
            channelItem.classList.add('drag-over');
        }
    }

    private handleDragLeave(e: DragEvent): void {
        const target = e.target as HTMLElement;
        const channelItem = target.closest('.channel-item') as HTMLElement;
        if (channelItem) {
            channelItem.classList.remove('drag-over');
        }
    }

    private handleDrop(e: DragEvent): void {
        e.preventDefault();
        const target = e.target as HTMLElement;
        const targetItem = target.closest('.channel-item') as HTMLElement;

        if (targetItem && this.draggedElement && targetItem !== this.draggedElement) {
            const targetRect = targetItem.getBoundingClientRect();
            const targetCenter = targetRect.top + targetRect.height / 2;

            if (e.clientY < targetCenter) {
                this.channelList.insertBefore(this.draggedElement, targetItem);
            } else {
                this.channelList.insertBefore(this.draggedElement, targetItem.nextSibling);
            }

            this.updateChannelPriorities();
        }

        targetItem?.classList.remove('drag-over');
    }

    private handleDragEnd(): void {
        if (this.draggedElement) {
            this.draggedElement.classList.remove('dragging');
            this.draggedElement = null;
        }

        // Remove all drag-over classes
        this.channelList.querySelectorAll('.channel-item').forEach(item => {
            item.classList.remove('drag-over');
        });
    }

    private updateChannelPriorities(): void {
        const channelItems = this.channelList.querySelectorAll('.channel-item');
        channelItems.forEach((item, index) => {
            const prioritySpan = item.querySelector('.channel-priority') as HTMLElement;
            if (prioritySpan) {
                prioritySpan.textContent = (index + 1).toString();
            }
        });
    }

    private async initializeWasm(): Promise<void> {
        try {
            this.showMessage('Initializing PNG sorter...', 'info');
            await this.pngSorter.init();
            this.showMessage('PNG sorter ready!', 'success');
            this.updateProcessButtonState();
            this.updateChannelOrderHint(); // Initialize the hint display
        } catch (error) {
            this.showMessage('Failed to initialize PNG sorter: ' + (error as Error).message, 'error');
        }
    }

    private handleImageSelect(event: Event): void {
        const target = event.target as HTMLInputElement;
        const file = target.files?.[0];

        if (file) {
            if (!file.type.match(/image\/png/)) {
                this.showMessage('Please select a PNG image file.', 'error');
                return;
            }

            this.currentImageFile = file;
            this.showMessage(`Selected: ${file.name}`, 'success');
            this.updateProcessButtonState();
        }
    }

    private async handleProcessImage(): Promise<void> {
        if (!this.currentImageFile) {
            this.showMessage('Please select an image first.', 'error');
            return;
        }

        try {
            this.setProcessingState(true);
            this.showMessage('Processing image...', 'info');

            // Get selected channels in the user-defined order
            const channels = this.getSelectedChannelsInOrder();

            if (channels.length === 0) {
                this.showMessage('Please select at least one color channel.', 'error');
                return;
            }

            // Build config
            const config: SortConfig = {
                descending: this.descending.checked,
                sort_range: this.sortRange.value as 'Row' | 'Column' | 'RowMajor' | 'ColumnMajor',
                sort_mode: this.sortMode.value as 'TiedBySum' | 'TiedByOrder' | 'Untied',
                sort_channel: channels
            };

            // Process the image
            const result = await this.pngSorter.processImage(this.currentImageFile, config);

            // Create blob and display result
            const resultArray = new Uint8Array(result);
            this.resultBlob = new Blob([resultArray], { type: 'image/png' });
            this.displayResult(this.resultBlob);
            this.showMessage('Image processed successfully!', 'success');

        } catch (error) {
            this.showMessage('Error processing image: ' + (error as Error).message, 'error');
        } finally {
            this.setProcessingState(false);
        }
    }

    private displayResult(blob: Blob): void {
        const displayUrl = URL.createObjectURL(blob);
        const img = document.createElement('img');
        img.src = displayUrl;
        img.className = 'image-preview';
        img.alt = 'Processed image';

        // Clear previous result
        this.resultContainer.innerHTML = '';
        this.resultContainer.appendChild(img);

        // Setup download with proper click handler
        this.downloadBtn.onclick = (e) => {
            e.preventDefault();
            this.downloadImage(blob);
        };
        this.downloadBtn.style.display = 'inline-block';

        // Cleanup display URL when image loads
        img.onload = () => URL.revokeObjectURL(displayUrl);
    }

    private downloadImage(blob: Blob): void {
        const downloadUrl = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = downloadUrl;
        link.download = `sorted_${this.currentImageFile?.name || 'image.png'}`;

        // Append to body, click, and remove
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);

        // Clean up the URL
        setTimeout(() => URL.revokeObjectURL(downloadUrl), 100);
    }

    private setProcessingState(isProcessing: boolean): void {
        this.processBtn.disabled = isProcessing;

        if (isProcessing) {
            this.processBtn.innerHTML = '<span class="loading"></span>Processing...';
        } else {
            this.processBtn.innerHTML = 'Process Image';
        }
    }

    private updateProcessButtonState(): void {
        const canProcess = this.currentImageFile && this.pngSorter &&
            this.pngSorter['isInitialized'];
        this.processBtn.disabled = !canProcess;
    }

    private showMessage(message: string, type: 'info' | 'success' | 'error'): void {
        const messageDiv = document.createElement('div');
        messageDiv.className = type;
        messageDiv.textContent = message;

        this.messageContainer.innerHTML = '';
        this.messageContainer.appendChild(messageDiv);

        // Auto-hide success and info messages
        if (type === 'success' || type === 'info') {
            setTimeout(() => {
                if (this.messageContainer.contains(messageDiv)) {
                    this.messageContainer.removeChild(messageDiv);
                }
            }, 3000);
        }
    }

    private updateChannelOrderHint(): void {
        const isTiedByOrder = this.sortMode.value === 'TiedByOrder';
        if (isTiedByOrder) {
            this.hintText.textContent = 'Higher priority channels sort first';
            this.hintText.style.fontWeight = '600';
            this.hintText.style.color = '#007acc';
        } else {
            this.hintText.textContent = 'Order only matters for "Tied By Order" mode';
            this.hintText.style.fontWeight = 'normal';
            this.hintText.style.color = '#666';
        }
    }

    private getSelectedChannelsInOrder(): string[] {
        const channels: string[] = [];
        const channelItems = this.channelList.querySelectorAll('.channel-item');

        channelItems.forEach(item => {
            const checkbox = item.querySelector('input[type="checkbox"]') as HTMLInputElement;
            if (checkbox && checkbox.checked) {
                const channel = item.getAttribute('data-channel');
                if (channel) {
                    channels.push(channel);
                }
            }
        });

        return channels;
    }
}

// Initialize the app when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    new PngSortApp();
});
