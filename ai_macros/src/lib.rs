use std::collections::HashMap;

use convert_case::{Case, Casing};
use proc_macro::TokenStream;
use proc_macro2::Group;
use quote::quote;
use syn::{parse_macro_input, Ident, FnArg, Pat, PatIdent, AttributeArgs, NestedMeta, Meta, ItemImpl};

#[proc_macro_attribute]
pub fn ai_functions(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut item_impl = parse_macro_input!(item as ItemImpl);

    let struct_ident = item_impl.self_ty.clone();
    let (impl_generics, ty_generics, where_clause) = item_impl.generics.split_for_impl();

    // Add two methods:
    // given a string function name, return a serde_json::Value JsonSchema for that function's arguments
    // given a string function name and a JSON object, call the function with the arguments from the JSON object

    let mut json_schema_branches = vec![];
    let mut json_call_branches = vec![];

    for item in item_impl.items.iter_mut() {
        if let syn::ImplItem::Method(method) = item {

            let fn_name = method.sig.ident.clone();

            method.attrs.retain_mut(|attr| {
                if attr.path.get_ident().unwrap().to_string() == "ai_function" {

                    let mut description = None;
                    let mut arg_descriptions = HashMap::new();

                    if let Ok(group) = syn::parse_macro_input::parse::<Group>(attr.tokens.clone().into()) {
                        if let Ok(attr_args) = syn::parse_macro_input::parse::<AttributeArgs>(group.stream().into()) {
                            for arg in attr_args {
                                match arg {
                                    NestedMeta::Meta(arg) => {
                                        match arg {
                                            Meta::NameValue(syn::MetaNameValue { path, lit: syn::Lit::Str(lit_str), .. }) => {
                                                let path = path.get_ident().unwrap().to_string();
                                                if path == "fn_description" {
                                                    description = Some(lit_str.value());
                                                } else {
                                                    arg_descriptions.insert(path.to_string(), lit_str.value());
                                                }
                                            }
                                            _ => todo!(),
                                        } 
                                    }
                                    NestedMeta::Lit(_) => todo!(),
                                }
                            }
                        }
                    }

                    let method_name = method.sig.ident.clone();
                    let method_str = method_name.to_string();
    
                    let mut schema_struct_fields = vec![];
                    let mut args_struct_fields = vec![];
                    let mut field_names = vec![];

                    for input in method.sig.inputs.iter() {
                        if let FnArg::Typed(arg) = input {
                            if let Pat::Ident(PatIdent { ident, .. }) = arg.pat.as_ref() {
                                let field_ident = Ident::new(&ident.to_string(), ident.span());
                                let field_type = arg.ty.clone();
                                let field_description = match arg_descriptions.get(&ident.to_string()) {
                                    Some(desc) => quote! { #[schemars(description = #desc)] },
                                    None => quote! {},
                                };
                                schema_struct_fields.push(quote! { #field_description #field_ident: #field_type });

                                // Create aliases for all the different possible cases, e.g. snake, camel, pascal
                                let mut serde_aliases = quote! {};
                                for case in [Case::Snake, Case::Camel, Case::Pascal] {
                                    let alias = ident.to_string().to_case(case);
                                    serde_aliases = quote! { #serde_aliases #[serde(alias = #alias)] };
                                }

                                args_struct_fields.push(quote! { #serde_aliases #field_ident: #field_type });
                                field_names.push(field_ident);
                            }
                        }
                    }

                    for field_name in arg_descriptions.keys() {
                        if !field_names.iter().any(|name| name.to_string() == *field_name) {
                            panic!("Field {} does not exist in function {}", field_name, fn_name);
                        }
                    }

                    let description = description.unwrap_or(method_str.clone());

                    let json_schema_branch = quote! {
                        #method_str => {
                            #[derive(JsonSchema)]
                            #[schemars(rename_all = "camelCase")]
                            #[allow(unused)]
                            struct Args {
                                #(#schema_struct_fields),*
                            }
                            
                            let parameters = ai_lib::schema::<Args>();

                            Some(ai_lib::Function {
                                name: #method_str.into(),
                                description: #description.into(),
                                parameters: parameters,
                            })
                        }
                    };
                    json_schema_branches.push(json_schema_branch);

                    let json_call_branch = quote! {
                        #method_str => {
                            #[derive(Deserialize)]
                            struct Args {
                                #(#args_struct_fields),*
                            }
    
                            let args: Args = serde_json::from_str(arg)?;
                            Self::#fn_name(self, #(args.#field_names),*)
                        }
                    };
                    json_call_branches.push(json_call_branch);

                    false
                } else {
                    true
                }
            });
        }
    }

    quote! {
        #item_impl

        impl #impl_generics ai_lib::AiState for #struct_ident #ty_generics #where_clause {
            fn json_schema_for_function(function_name: &str) -> Option<ai_lib::Function> {
                match function_name {
                    #(#json_schema_branches),*
                    _ => None,
                }
            }

            fn call_function(&mut self, function_name: &str, arg: &str) -> ai_lib::AiFunctionResult {
                match function_name {
                    #(#json_call_branches),*
                    _ => ai_lib::recoverable_err(format!("Function {function_name} not found"))
                }
            }
        }
    }.into()
}
