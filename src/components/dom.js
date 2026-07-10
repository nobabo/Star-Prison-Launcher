export function fragment(...children){
    const node = document.createDocumentFragment()
    appendChildren(node, children)
    return node
}

export function el(tagName, options = {}, ...children){
    const node = document.createElement(tagName)
    const {
        className,
        text,
        attrs = {},
        dataset = {},
        style = {},
        props = {}
    } = options

    if(className != null){
        node.className = className
    }

    for(const [name, value] of Object.entries(attrs)){
        if(value != null){
            node.setAttribute(name, String(value))
        }
    }

    for(const [name, value] of Object.entries(dataset)){
        if(value != null){
            node.dataset[name] = String(value)
        }
    }

    for(const [name, value] of Object.entries(style)){
        if(value != null){
            node.style.setProperty(name, String(value))
        }
    }

    for(const [name, value] of Object.entries(props)){
        node[name] = value
    }

    if(text != null){
        node.textContent = String(text)
    }

    appendChildren(node, children)
    return node
}

export function appendChildren(parent, children){
    for(const child of children.flat()){
        if(child == null || child === false){
            continue
        }

        if(child instanceof Node){
            parent.appendChild(child)
            continue
        }

        parent.appendChild(document.createTextNode(String(child)))
    }
}

export function iconImage(src){
    return el('img', {
        className: 'button-icon',
        attrs: {
            src,
            alt: '',
            'aria-hidden': 'true'
        }
    })
}
